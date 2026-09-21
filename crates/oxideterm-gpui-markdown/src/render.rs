// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Render a [`MarkdownDocument`] into a GPUI element tree.
//!
//! This module converts OxideTerm-owned markdown model nodes into composed
//! GPUI `Div` / `AnyElement` trees using only semantic theme tokens.

use std::{ops::Range, path::PathBuf, rc::Rc, sync::Arc};

use gpui::{
    AnyElement, App, ClipboardItem, ElementId, Font, FontFeatures, FontStyle, FontWeight, Hsla,
    Image, InteractiveElement, IntoElement, MouseButton, ParentElement, SharedString,
    StatefulInteractiveElement, StrikethroughStyle, Styled, StyledImage, StyledText, TextAlign,
    TextRun, UnderlineStyle, Window, div, image_cache, img, prelude::FluentBuilder, px, relative,
    retain_all,
};
use oxideterm_gpui_ui::{ScrollableElement, Scrollbar};
use oxideterm_theme::ThemeTokens;

use crate::MarkdownVirtualListScrollHandle;
use crate::highlight;
use crate::layout::{MarkdownBlockLayout, MarkdownLayoutItem};
use crate::math;
use crate::mermaid;
use crate::model::{
    Block, BlockAlignment, CalloutKind, FootnoteDefinition, ImageDimensions, ImageLength, Inline,
    ListItem, MarkdownDocument, TableAlignment,
};
use crate::options::MarkdownOptions;
use crate::style;

const WINDOWED_MARKDOWN_MIN_ITEMS: usize = 24;
const MARKDOWN_VIRTUAL_OVERDRAW_PX: f32 = 480.0;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct MarkdownTextFragmentId {
    pub key: String,
    pub join_previous: bool,
}

impl From<String> for MarkdownTextFragmentId {
    fn from(key: String) -> Self {
        Self {
            key,
            join_previous: false,
        }
    }
}

#[derive(Clone)]
pub struct MarkdownTextLink {
    pub range: Range<usize>,
    pub open: Rc<dyn Fn(&mut Window, &mut App)>,
}

pub type MarkdownCodeRunHandler = Arc<dyn Fn(String, &mut Window, &mut App) + 'static>;
pub type MarkdownMermaidZoomHandler =
    Arc<dyn Fn(String, Arc<Image>, f32, f32, &mut Window, &mut App) + 'static>;

#[derive(Clone, Default)]
pub struct MarkdownCodeBlockActions {
    pub on_run: Option<MarkdownCodeRunHandler>,
    pub on_mermaid_zoom: Option<MarkdownMermaidZoomHandler>,
}

/// Render a complete markdown document into a vertical GPUI container.
pub fn render_document(
    document: &MarkdownDocument,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    render_document_with_code_actions(document, tokens, opts, None)
}

fn render_document_with_code_actions(
    document: &MarkdownDocument,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    let mut content = div()
        .line_height(relative(style::BODY_LINE_HEIGHT))
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(opts.block_gap))
        .child(render_blocks_with_code_actions(
            &document.blocks,
            tokens,
            opts,
            code_actions,
        ));

    if opts.enable_footnotes && !document.footnotes.is_empty() {
        content = content.child(render_footnotes(&document.footnotes, tokens, opts));
    }

    if opts.enable_async_images {
        image_cache(retain_all(opts.image_cache_id))
            .child(content)
            .into_any_element()
    } else {
        content.into_any_element()
    }
}

/// Render a markdown document by keeping its estimated full height while only
/// building GPUI elements for blocks near the visible portion.
pub fn render_document_windowed(
    document: &MarkdownDocument,
    layout: &MarkdownBlockLayout,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    viewport_top: f32,
    viewport_height: f32,
    overdraw: f32,
) -> AnyElement {
    render_document_windowed_with_code_actions(
        document,
        layout,
        tokens,
        opts,
        viewport_top,
        viewport_height,
        overdraw,
        None,
    )
}

fn render_document_windowed_with_code_actions(
    document: &MarkdownDocument,
    layout: &MarkdownBlockLayout,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    viewport_top: f32,
    viewport_height: f32,
    overdraw: f32,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    let mut options = opts.clone();
    options.disclosures = layout.disclosures.clone();
    let opts = &options;
    let items = layout.items();
    if items.len() < WINDOWED_MARKDOWN_MIN_ITEMS || viewport_height <= 0.0 {
        return render_document_with_code_actions(document, tokens, opts, code_actions);
    }

    let item_sizes = layout.item_sizes();
    let total_height = estimated_markdown_height(&item_sizes, opts.block_gap);
    if total_height <= viewport_height + overdraw * 2.0 {
        return render_document_with_code_actions(document, tokens, opts, code_actions);
    }

    let Some(virtual_window) = markdown_virtual_window(
        &item_sizes,
        opts.block_gap,
        viewport_top,
        viewport_height,
        overdraw,
    ) else {
        return render_document_with_code_actions(document, tokens, opts, code_actions);
    };
    let mut rendered = Vec::new();

    for (index, item) in items
        .iter()
        .enumerate()
        .skip(virtual_window.range.start)
        .take(virtual_window.range.len())
    {
        match item {
            MarkdownLayoutItem::Block(block) => {
                rendered.push(
                    layout.measure(
                        index,
                        div()
                            .id(("markdown-block", index))
                            .w_full()
                            .min_w_0()
                            .pt(px(style::block_top_padding(block, index, opts)))
                            .child(render_block_with_code_actions(
                                block,
                                tokens,
                                opts,
                                code_actions,
                            ))
                            .into_any_element(),
                    ),
                );
            }
            MarkdownLayoutItem::Footnotes(footnotes) => {
                rendered.push(layout.measure(index, render_footnotes(footnotes, tokens, opts)));
            }
        }
    }

    if rendered.is_empty() {
        let content = div().w_full().min_w_0().h(px(total_height));
        return if opts.enable_async_images {
            image_cache(retain_all(opts.image_cache_id))
                .child(content)
                .into_any_element()
        } else {
            content.into_any_element()
        };
    }

    let mut content = div()
        .line_height(relative(style::BODY_LINE_HEIGHT))
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(opts.block_gap));
    if virtual_window.top_spacer > 0.0 {
        content = content.child(
            div()
                .w_full()
                .h(px((virtual_window.top_spacer - opts.block_gap).max(0.0))),
        );
    }
    content = content.children(rendered);
    if virtual_window.bottom_spacer > 0.0 {
        content = content.child(
            div()
                .w_full()
                .h(px((virtual_window.bottom_spacer - opts.block_gap).max(0.0))),
        );
    }

    if opts.enable_async_images {
        image_cache(retain_all(opts.image_cache_id))
            .child(content)
            .into_any_element()
    } else {
        content.into_any_element()
    }
}

pub fn render_document_selectable(
    document: &MarkdownDocument,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    render_document_selectable_with_code_actions(document, tokens, opts, None, render_text)
}

pub fn render_document_selectable_with_code_actions(
    document: &MarkdownDocument,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    let mut content = div()
        .line_height(relative(style::BODY_LINE_HEIGHT))
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(opts.block_gap))
        .child(render_selectable_blocks(
            &document.blocks,
            tokens,
            opts,
            code_actions,
            "b",
            render_text,
        ));

    if opts.enable_footnotes && !document.footnotes.is_empty() {
        content = content.child(render_footnotes(&document.footnotes, tokens, opts));
    }

    if opts.enable_async_images {
        image_cache(retain_all(opts.image_cache_id))
            .child(content)
            .into_any_element()
    } else {
        content.into_any_element()
    }
}

pub fn render_document_windowed_selectable(
    document: &MarkdownDocument,
    layout: &MarkdownBlockLayout,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    viewport_top: f32,
    viewport_height: f32,
    overdraw: f32,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    render_document_windowed_selectable_with_code_actions(
        document,
        layout,
        tokens,
        opts,
        viewport_top,
        viewport_height,
        overdraw,
        None,
        render_text,
    )
}

pub fn render_document_windowed_selectable_with_code_actions(
    document: &MarkdownDocument,
    layout: &MarkdownBlockLayout,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    viewport_top: f32,
    viewport_height: f32,
    overdraw: f32,
    code_actions: Option<&MarkdownCodeBlockActions>,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    let mut options = opts.clone();
    options.disclosures = layout.disclosures.clone();
    let opts = &options;
    let items = layout.items();
    if items.len() < WINDOWED_MARKDOWN_MIN_ITEMS || viewport_height <= 0.0 {
        return render_document_selectable_with_code_actions(
            document,
            tokens,
            opts,
            code_actions,
            render_text,
        );
    }

    let item_sizes = layout.item_sizes();
    let total_height = estimated_markdown_height(&item_sizes, opts.block_gap);
    if total_height <= viewport_height + overdraw * 2.0 {
        return render_document_selectable_with_code_actions(
            document,
            tokens,
            opts,
            code_actions,
            render_text,
        );
    }

    let Some(virtual_window) = markdown_virtual_window(
        &item_sizes,
        opts.block_gap,
        viewport_top,
        viewport_height,
        overdraw,
    ) else {
        return render_document_selectable_with_code_actions(
            document,
            tokens,
            opts,
            code_actions,
            render_text,
        );
    };
    let mut rendered = Vec::new();

    for (index, item) in items
        .iter()
        .enumerate()
        .skip(virtual_window.range.start)
        .take(virtual_window.range.len())
    {
        match item {
            MarkdownLayoutItem::Block(block) => {
                rendered.push(
                    layout.measure(
                        index,
                        div()
                            .id(("markdown-block", index))
                            .w_full()
                            .min_w_0()
                            .pt(px(style::block_top_padding(block, index, opts)))
                            .child(render_selectable_block(
                                block,
                                tokens,
                                opts,
                                code_actions,
                                &format!("w:{index}"),
                                render_text,
                            ))
                            .into_any_element(),
                    ),
                );
            }
            MarkdownLayoutItem::Footnotes(footnotes) => {
                rendered.push(layout.measure(index, render_footnotes(footnotes, tokens, opts)));
            }
        }
    }

    if rendered.is_empty() {
        let content = div().w_full().min_w_0().h(px(total_height));
        return if opts.enable_async_images {
            image_cache(retain_all(opts.image_cache_id))
                .child(content)
                .into_any_element()
        } else {
            content.into_any_element()
        };
    }

    let mut content = div()
        .line_height(relative(style::BODY_LINE_HEIGHT))
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(opts.block_gap));
    if virtual_window.top_spacer > 0.0 {
        content = content.child(
            div()
                .w_full()
                .h(px((virtual_window.top_spacer - opts.block_gap).max(0.0))),
        );
    }
    content = content.children(rendered);
    if virtual_window.bottom_spacer > 0.0 {
        content = content.child(
            div()
                .w_full()
                .h(px((virtual_window.bottom_spacer - opts.block_gap).max(0.0))),
        );
    }

    if opts.enable_async_images {
        image_cache(retain_all(opts.image_cache_id))
            .child(content)
            .into_any_element()
    } else {
        content.into_any_element()
    }
}

/// Render a complete markdown document through a block-level virtual list.
pub fn render_document_virtual(
    id: impl Into<ElementId>,
    document: &MarkdownDocument,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    scroll_handle: &MarkdownVirtualListScrollHandle,
) -> AnyElement {
    render_document_virtual_with_code_actions(id, document, tokens, opts, scroll_handle, None)
}

pub fn render_document_virtual_with_code_actions(
    id: impl Into<ElementId>,
    document: &MarkdownDocument,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    scroll_handle: &MarkdownVirtualListScrollHandle,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    let layout = scroll_handle.measurements.prepare(
        MarkdownBlockLayout::from_document(document, opts),
        f32::from(scroll_handle.bounds().size.width),
        opts,
    );
    let mut options = opts.clone();
    options.disclosures = layout.disclosures.clone();
    options
        .navigation
        .get_or_insert_with(|| scroll_handle.navigation.clone());
    let opts = &options;
    let viewport_top = opts.scroll_sync.as_ref().map_or_else(
        || markdown_scroll_top_from_gpui_offset(scroll_handle.offset().y),
        |sync| sync.begin(&layout, opts.block_gap, scroll_handle),
    );
    if let Some(navigation) = &opts.navigation {
        navigation.prepare(document, opts);
    }
    let viewport_height = f32::from(scroll_handle.bounds().size.height);
    let content = render_document_windowed_with_code_actions(
        document,
        &layout,
        tokens,
        opts,
        viewport_top,
        viewport_height,
        MARKDOWN_VIRTUAL_OVERDRAW_PX,
        code_actions,
    );

    // GPUI's built-in ScrollHandle keeps the same owner model without pulling
    // in an external variable-height list for markdown previews.
    div()
        .id(id)
        .relative()
        .size_full()
        .child(
            div()
                .id("markdown-viewport")
                .size_full()
                .overflow_y_scroll()
                .restrict_scroll_to_axis()
                .track_scroll(scroll_handle)
                .child(content),
        )
        .child(
            Scrollbar::new(scroll_handle).when_some(opts.scroll_sync.as_ref(), |bar, sync| {
                let sync = sync.clone();
                bar.on_vertical_scroll(move || sync.user_input())
            }),
        )
        .when_some(opts.scroll_sync.as_ref(), |root, sync| {
            let input = sync.clone();
            root.on_scroll_wheel(move |_, _, _| input.user_input())
                .child(sync.finish_probe(scroll_handle))
        })
        .into_any_element()
}

pub fn render_document_virtual_selectable(
    id: impl Into<ElementId>,
    document: &MarkdownDocument,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    scroll_handle: &MarkdownVirtualListScrollHandle,
    code_actions: Option<&MarkdownCodeBlockActions>,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    let layout = scroll_handle.measurements.prepare(
        MarkdownBlockLayout::from_document(document, opts),
        f32::from(scroll_handle.bounds().size.width),
        opts,
    );
    let mut options = opts.clone();
    options.disclosures = layout.disclosures.clone();
    options
        .navigation
        .get_or_insert_with(|| scroll_handle.navigation.clone());
    let opts = &options;
    let viewport_top = opts.scroll_sync.as_ref().map_or_else(
        || markdown_scroll_top_from_gpui_offset(scroll_handle.offset().y),
        |sync| sync.begin(&layout, opts.block_gap, scroll_handle),
    );
    if let Some(navigation) = &opts.navigation {
        navigation.prepare(document, opts);
    }
    let content = render_document_windowed_selectable_with_code_actions(
        document,
        &layout,
        tokens,
        opts,
        viewport_top,
        f32::from(scroll_handle.bounds().size.height),
        MARKDOWN_VIRTUAL_OVERDRAW_PX,
        code_actions,
        render_text,
    );
    div()
        .id(id)
        .relative()
        .size_full()
        .child(
            div()
                .id("markdown-viewport")
                .size_full()
                .overflow_y_scroll()
                .restrict_scroll_to_axis()
                .track_scroll(scroll_handle)
                .child(content),
        )
        .child(
            Scrollbar::new(scroll_handle).when_some(opts.scroll_sync.as_ref(), |bar, sync| {
                let sync = sync.clone();
                bar.on_vertical_scroll(move || sync.user_input())
            }),
        )
        .when_some(opts.scroll_sync.as_ref(), |root, sync| {
            let input = sync.clone();
            root.on_scroll_wheel(move |_, _, _| input.user_input())
                .child(sync.finish_probe(scroll_handle))
        })
        .into_any_element()
}

fn estimated_markdown_height(item_sizes: &[gpui::Size<gpui::Pixels>], block_gap: f32) -> f32 {
    let items_height: f32 = item_sizes.iter().map(|size| f32::from(size.height)).sum();
    items_height + block_gap * item_sizes.len().saturating_sub(1) as f32
}

#[derive(Clone, Debug, PartialEq)]
struct MarkdownVirtualWindow {
    range: Range<usize>,
    top_spacer: f32,
    bottom_spacer: f32,
}

fn markdown_virtual_window(
    item_sizes: &[gpui::Size<gpui::Pixels>],
    block_gap: f32,
    viewport_top: f32,
    viewport_height: f32,
    overdraw: f32,
) -> Option<MarkdownVirtualWindow> {
    if item_sizes.is_empty() {
        return None;
    }

    let total_height = estimated_markdown_height(item_sizes, block_gap);
    if total_height <= 0.0 {
        return None;
    }

    let viewport_top = finite_non_negative(viewport_top);
    let viewport_height = finite_non_negative(viewport_height);
    let overdraw = finite_non_negative(overdraw);
    let block_gap = finite_non_negative(block_gap);
    let max_viewport_top = (total_height - viewport_height).max(0.0);
    let clamped_viewport_top = viewport_top.min(max_viewport_top);
    let window_top = (clamped_viewport_top - overdraw).max(0.0);
    let window_bottom = (clamped_viewport_top + viewport_height + overdraw).min(total_height);

    // Keep the item origins as the source of truth, like a variable-height
    // list. This prevents an empty render window when the scroll offset and
    // estimated markdown height drift apart.
    let mut item_bounds = Vec::with_capacity(item_sizes.len());
    let mut cursor_y = 0.0;
    for (index, size) in item_sizes.iter().enumerate() {
        let item_top = cursor_y;
        let item_bottom = item_top + finite_non_negative(f32::from(size.height));
        item_bounds.push((item_top, item_bottom));
        cursor_y = item_bottom;
        if index + 1 < item_sizes.len() {
            cursor_y += block_gap;
        }
    }

    let mut first_index = None;
    let mut last_index_exclusive = 0;
    for (index, (item_top, item_bottom)) in item_bounds.iter().copied().enumerate() {
        if item_bottom >= window_top && item_top <= window_bottom {
            first_index.get_or_insert(index);
            last_index_exclusive = index + 1;
        }
    }

    let (first_index, last_index_exclusive) = match first_index {
        Some(first_index) => (first_index, last_index_exclusive),
        None => {
            let fallback_index = item_bounds
                .iter()
                .position(|(_, item_bottom)| *item_bottom >= clamped_viewport_top)
                .unwrap_or_else(|| item_bounds.len().saturating_sub(1));
            (fallback_index, fallback_index + 1)
        }
    };

    let (first_item_top, _) = item_bounds[first_index];
    let (_, last_item_bottom) = item_bounds[last_index_exclusive - 1];

    Some(MarkdownVirtualWindow {
        range: first_index..last_index_exclusive,
        // Keep content height independent of overscroll so the scroll container
        // can clamp stale offsets after measurements or disclosure changes.
        top_spacer: first_item_top,
        bottom_spacer: (total_height - last_item_bottom).max(0.0),
    })
}

fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn markdown_scroll_top_from_gpui_offset(offset_y: gpui::Pixels) -> f32 {
    // GPUI scroll offsets move the child upward, so scrolling down makes the
    // y offset negative. Markdown virtualization needs a positive scroll top.
    finite_non_negative(-f32::from(offset_y))
}

/// Render a list of blocks into a vertical GPUI container.
pub fn render_blocks(blocks: &[Block], tokens: &ThemeTokens, opts: &MarkdownOptions) -> AnyElement {
    render_blocks_with_code_actions(blocks, tokens, opts, None)
}

fn render_blocks_with_code_actions(
    blocks: &[Block],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(opts.block_gap))
        .children(blocks.iter().enumerate().map(|(index, block)| {
            div()
                .id(("markdown-block", index))
                .w_full()
                .min_w_0()
                .pt(px(style::block_top_padding(block, index, opts)))
                .child(render_block_with_code_actions(
                    block,
                    tokens,
                    opts,
                    code_actions,
                ))
        }))
        .into_any_element()
}

fn render_selectable_blocks(
    blocks: &[Block],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
    path: &str,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(opts.block_gap))
        .children(blocks.iter().enumerate().map(|(index, block)| {
            div()
                .id(SharedString::from(format!("{path}:{index}")))
                .w_full()
                .min_w_0()
                .pt(px(style::block_top_padding(block, index, opts)))
                .child(render_selectable_block(
                    block,
                    tokens,
                    opts,
                    code_actions,
                    &format!("{path}:{index}"),
                    render_text,
                ))
        }))
        .into_any_element()
}

fn wrap_source_block(
    span: crate::model::SourceSpan,
    block: &Block,
    child: AnyElement,
    opts: &MarkdownOptions,
) -> AnyElement {
    let Some(sync) = &opts.scroll_sync else {
        return child;
    };
    if let Block::CodeBlock { language, code } = block
        && !should_render_mermaid_block(language.as_deref(), code)
    {
        // The code body's own probe excludes the action bar and padding.
        return child;
    }
    let collapsed =
        matches!(block, Block::Details { id, open, .. } if !opts.disclosures.is_open(id, *open));
    sync.wrap_visibility(span, child, collapsed)
}

fn render_block_with_code_actions(
    block: &Block,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    match block {
        Block::Located { span, block } => {
            let mut options = opts.clone();
            if matches!(block.as_ref(), Block::CodeBlock { .. }) {
                options.code_source = Some(*span);
            }
            let child = render_block_with_code_actions(block, tokens, &options, code_actions);
            wrap_source_block(*span, block, child, opts)
        }
        Block::Heading { level, id, inlines } => render_heading(*level, id, inlines, tokens, opts),
        Block::Paragraph { inlines } => render_paragraph(inlines, tokens, opts),
        Block::Html(html) => render_html_block(html, tokens, opts),
        Block::HtmlContainer { alignment, blocks } => {
            render_html_container(*alignment, blocks, tokens, opts, code_actions)
        }
        Block::Details {
            id,
            summary,
            blocks,
            open,
        } => crate::disclosure::HtmlDisclosure {
            id: id.clone(),
            summary: render_paragraph(&details_summary(summary, opts), tokens, opts),
            body: render_blocks_with_code_actions(blocks, tokens, opts, code_actions),
            default_open: *open,
            state: opts.disclosures.clone(),
            tokens: *tokens,
        }
        .into_any_element(),
        Block::CodeBlock { language, code } => {
            render_code_block(language.as_deref(), code, tokens, opts, code_actions)
        }
        Block::UnorderedList { items } => render_unordered_list(items, tokens, opts, code_actions),
        Block::OrderedList { start, items } => {
            render_ordered_list(*start, items, tokens, opts, code_actions)
        }
        Block::HorizontalRule => render_hr(tokens),
        Block::Blockquote { kind, blocks } => {
            render_blockquote_with_code_actions(*kind, blocks, tokens, opts, code_actions)
        }
        Block::Table {
            headers,
            alignments,
            rows,
        } => render_table(headers, alignments, rows, tokens, opts),
    }
}

fn render_selectable_block(
    block: &Block,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
    path: &str,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    match block {
        Block::Located { span, block } => {
            let mut options = opts.clone();
            if matches!(block.as_ref(), Block::CodeBlock { .. }) {
                options.code_source = Some(*span);
            }
            let child =
                render_selectable_block(block, tokens, &options, code_actions, path, render_text);
            wrap_source_block(*span, block, child, opts)
        }
        Block::Heading { level, id, inlines } => {
            render_selectable_heading(*level, id, inlines, tokens, opts, path, render_text)
        }
        Block::Paragraph { inlines } => {
            render_selectable_paragraph(inlines, tokens, opts, path, render_text)
        }
        Block::Html(html) => render_selectable_html_block(html, tokens, opts, path, render_text),
        Block::HtmlContainer { alignment, blocks } => render_selectable_html_container(
            *alignment,
            blocks,
            tokens,
            opts,
            code_actions,
            path,
            render_text,
        ),
        Block::Details {
            id,
            summary,
            blocks,
            open,
        } => crate::disclosure::HtmlDisclosure {
            id: id.clone(),
            summary: render_paragraph(&details_summary(summary, opts), tokens, opts),
            body: render_selectable_blocks(
                blocks,
                tokens,
                opts,
                code_actions,
                &format!("{path}:details"),
                render_text,
            ),
            default_open: *open,
            state: opts.disclosures.clone(),
            tokens: *tokens,
        }
        .into_any_element(),
        Block::CodeBlock { language, code } => render_selectable_code_block(
            language.as_deref(),
            code,
            tokens,
            opts,
            code_actions,
            path,
            render_text,
        ),
        Block::UnorderedList { items } => {
            render_selectable_unordered_list(items, tokens, opts, code_actions, path, render_text)
        }
        Block::OrderedList { start, items } => render_selectable_ordered_list(
            *start,
            items,
            tokens,
            opts,
            code_actions,
            path,
            render_text,
        ),
        Block::HorizontalRule => render_hr(tokens),
        Block::Blockquote { kind, blocks } => render_selectable_blockquote(
            *kind,
            blocks,
            tokens,
            opts,
            code_actions,
            path,
            render_text,
        ),
        Block::Table {
            headers,
            alignments,
            rows,
        } => render_selectable_table(headers, alignments, rows, tokens, opts, path, render_text),
    }
}

// ─── headings ───────────────────────────────────────────────────────────

fn details_summary(summary: &[Inline], opts: &MarkdownOptions) -> Vec<Inline> {
    if summary.is_empty() {
        vec![Inline::Text(opts.html_details_label.clone())]
    } else {
        summary.to_vec()
    }
}

fn render_heading(
    level: u8,
    id: &str,
    inlines: &[Inline],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    let font_size = style::heading_font_size(level, opts);
    div()
        .relative()
        .child(heading_anchor(id, opts))
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .min_w_0()
                .whitespace_normal()
                .text_size(font_size)
                .line_height(relative(1.25))
                .text_color(style::heading_color(tokens))
                .child(render_styled_inlines_with_style(
                    inlines,
                    tokens,
                    opts,
                    FlatRunStyle {
                        semibold: true,
                        ..Default::default()
                    },
                )),
        )
        .into_any_element()
}

fn render_selectable_heading(
    level: u8,
    id: &str,
    inlines: &[Inline],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    path: &str,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    let font_size = style::heading_font_size(level, opts);
    div()
        .relative()
        .child(heading_anchor(id, opts))
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .min_w_0()
                .whitespace_normal()
                .text_size(font_size)
                .line_height(relative(1.25))
                .text_color(style::heading_color(tokens))
                .child(render_selectable_inlines_with_style(
                    path,
                    inlines,
                    tokens,
                    opts,
                    FlatRunStyle {
                        semibold: true,
                        ..Default::default()
                    },
                    render_text,
                )),
        )
        .into_any_element()
}

// ─── paragraphs ─────────────────────────────────────────────────────────

fn render_paragraph(
    inlines: &[Inline],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .whitespace_normal()
        .text_size(style::body_font_size(opts))
        .text_color(style::text_color(tokens))
        .child(render_styled_inlines(inlines, tokens, opts))
        .into_any_element()
}

fn render_selectable_paragraph(
    inlines: &[Inline],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    path: &str,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .whitespace_normal()
        .text_size(style::body_font_size(opts))
        .text_color(style::text_color(tokens))
        .child(render_selectable_inlines(
            path,
            inlines,
            tokens,
            opts,
            render_text,
        ))
        .into_any_element()
}

fn render_html_block(html: &str, tokens: &ThemeTokens, opts: &MarkdownOptions) -> AnyElement {
    // Raw HTML stays visible as inert text; GPUI native markdown intentionally
    // does not execute or interpret embedded HTML.
    render_paragraph(&[Inline::Html(html.to_string())], tokens, opts)
}

fn render_selectable_html_block(
    html: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    path: &str,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    render_selectable_paragraph(
        &[Inline::Html(html.to_string())],
        tokens,
        opts,
        path,
        render_text,
    )
}

fn render_html_container(
    alignment: BlockAlignment,
    blocks: &[Block],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .text_align(block_alignment_text_align(alignment))
        .child(render_blocks_with_code_actions(
            blocks,
            tokens,
            opts,
            code_actions,
        ))
        .into_any_element()
}

fn render_selectable_html_container(
    alignment: BlockAlignment,
    blocks: &[Block],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
    path: &str,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .text_align(block_alignment_text_align(alignment))
        .child(render_selectable_blocks(
            blocks,
            tokens,
            opts,
            code_actions,
            path,
            render_text,
        ))
        .into_any_element()
}

// ─── code blocks ────────────────────────────────────────────────────────

fn code_display_text(code: &str) -> &str {
    // The parser includes the newline before the closing fence; it is not an extra visual row.
    code.strip_suffix("\r\n")
        .or_else(|| code.strip_suffix('\n'))
        .unwrap_or(code)
}

fn render_code_block(
    language: Option<&str>,
    code: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    if should_render_mermaid_block(language, code) {
        return render_mermaid_block(code, tokens, opts, code_actions);
    }

    let display_code = code_display_text(code);
    // Attempt syntax highlighting; fall back to plain monospace text.
    let code_element: AnyElement = if let Some(lang) = language {
        if let Some(runs) = highlight::highlight_code(lang, display_code, opts) {
            let (text, text_runs) = highlight::highlighted_runs_to_text_runs(&runs);
            StyledText::new(text)
                .with_runs(text_runs)
                .into_any_element()
        } else {
            SharedString::from(display_code.to_string()).into_any_element()
        }
    } else {
        SharedString::from(display_code.to_string()).into_any_element()
    };

    render_code_block_shell(language, code, tokens, opts, None, code_element).into_any_element()
}

fn render_selectable_code_block(
    language: Option<&str>,
    code: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
    path: &str,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    if should_render_mermaid_block(language, code) {
        return render_mermaid_block(code, tokens, opts, code_actions);
    }

    let display_code = code_display_text(code);
    let code_element: AnyElement = if let Some(lang) = language {
        if let Some(runs) = highlight::highlight_code(lang, display_code, opts) {
            let (text, text_runs) = highlight::highlighted_runs_to_text_runs(&runs);
            render_text(format!("{path}:code").into(), text, text_runs, Vec::new())
        } else {
            render_text(
                format!("{path}:code").into(),
                SharedString::from(display_code.to_string()),
                vec![plain_code_run(display_code, tokens, opts)],
                Vec::new(),
            )
        }
    } else {
        render_text(
            format!("{path}:code").into(),
            SharedString::from(display_code.to_string()),
            vec![plain_code_run(display_code, tokens, opts)],
            Vec::new(),
        )
    };

    render_code_block_shell(language, code, tokens, opts, code_actions, code_element)
        .into_any_element()
}

fn render_mermaid_block(
    code: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    mermaid::MermaidBlock {
        source: code.to_string(),
        tokens: *tokens,
        options: opts.clone(),
        actions: code_actions.cloned(),
    }
    .into_any_element()
}

pub(crate) fn render_mermaid_header(
    code: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
    rendered: Option<&mermaid::RenderedMermaidImage>,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px(px(8.0))
        .py(px(4.0))
        .border_b_1()
        .border_color(style::code_block_header_border_color(tokens))
        .bg(style::code_block_header_bg_color(tokens, opts))
        // Mermaid uses the same painted shell as code blocks; the header owns
        // its top radius so GPUI cannot leak rectangular child backgrounds.
        .rounded_t(px(tokens.radii.md))
        .child(
            div()
                .text_size(style::code_label_font_size(opts))
                .text_color(style::muted_color(tokens))
                .font(style::code_font(opts))
                .child(SharedString::from("MERMAID")),
        )
        .child(render_mermaid_actions(
            code,
            tokens,
            opts,
            code_actions,
            rendered,
        ))
        .into_any_element()
}

fn render_mermaid_actions(
    code: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
    rendered: Option<&mermaid::RenderedMermaidImage>,
) -> AnyElement {
    let mut actions = div().flex().flex_row().items_center().gap(px(10.0));
    if let (Some(rendered), Some(on_zoom)) = (
        rendered,
        code_actions.and_then(|actions| actions.on_mermaid_zoom.clone()),
    ) {
        actions = actions.child(render_mermaid_zoom_action(
            code, tokens, opts, rendered, on_zoom,
        ));
    }

    actions.into_any_element()
}

fn render_mermaid_zoom_action(
    code: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    rendered: &mermaid::RenderedMermaidImage,
    on_zoom: MarkdownMermaidZoomHandler,
) -> AnyElement {
    let code = code.to_string();
    let image = rendered.image.clone();
    let width = rendered.display_width;
    let height = rendered.display_height;
    let hover_color = style::accent_color(tokens);

    render_code_action_label(opts.mermaid_expand_label.clone(), tokens, opts, hover_color)
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            // The app owns modal state; markdown only passes the already-rendered SVG image.
            on_zoom(code.clone(), image.clone(), width, height, window, cx);
            cx.stop_propagation();
        })
        .into_any_element()
}

pub(crate) fn render_mermaid_body(
    code: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    rendered: Result<mermaid::RenderedMermaidImage, String>,
) -> AnyElement {
    match rendered {
        Ok(rendered) => div()
            .w_full()
            .min_w_0()
            .p(px(opts.code_block_padding))
            .flex()
            .justify_center()
            .child(
                img(rendered.image)
                    .w(px(rendered.display_width))
                    .max_w(relative(1.0)),
            )
            .into_any_element(),
        Err(error) => div()
            .w_full()
            .min_w_0()
            .p(px(opts.code_block_padding))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .rounded(px(tokens.radii.sm))
                    .border_1()
                    .border_color(style::code_block_border_color(tokens))
                    .bg(style::code_bg_color(tokens, opts))
                    .p(px(8.0))
                    .text_size(style::code_font_size(opts))
                    .text_color(style::muted_color(tokens))
                    .font(style::code_font(opts))
                    .child(SharedString::from(format!(
                        "{}: {error}",
                        opts.mermaid_error_prefix
                    ))),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_size(style::code_font_size(opts))
                    .text_color(style::text_color(tokens))
                    .font(style::code_font(opts))
                    .child(SharedString::from(code.to_string())),
            )
            .into_any_element(),
    }
}

fn render_code_block_shell(
    language: Option<&str>,
    code: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
    code_element: AnyElement,
) -> gpui::Div {
    div()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .border_1()
        .border_color(style::code_block_border_color(tokens))
        .bg(style::code_block_bg_color(tokens, opts))
        .rounded(px(tokens.radii.md))
        .child(render_code_block_header(
            language,
            code,
            tokens,
            opts,
            code_actions,
        ))
        .child(
            div()
                .id("markdown-code-body")
                .w_full()
                .min_w_0()
                .whitespace_nowrap()
                .restrict_scroll_to_axis()
                .overflow_x_scrollbar()
                .p(px(opts.code_block_padding))
                .text_size(style::code_font_size(opts))
                .line_height(relative(1.5))
                .text_color(style::text_color(tokens))
                .font(style::code_font(opts))
                .child(
                    if let (Some(sync), Some(span)) = (&opts.scroll_sync, opts.code_source) {
                        sync.wrap(span, code_element)
                    } else {
                        code_element
                    },
                ),
        )
}

fn render_code_block_header(
    language: Option<&str>,
    code: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    div()
        .line_height(relative(1.3))
        .w_full()
        .min_w_0()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px(px(8.0))
        .py(px(4.0))
        .border_b_1()
        .border_color(style::code_block_header_border_color(tokens))
        .bg(style::code_block_header_bg_color(tokens, opts))
        // GPUI does not always clip child backgrounds to the parent radius;
        // Tauri relies on md-code-block overflow-hidden, so mirror that by
        // rounding the painted header corners explicitly.
        .rounded_t(px(tokens.radii.md))
        .child(
            div()
                .text_size(style::code_label_font_size(opts))
                .text_color(style::muted_color(tokens))
                .font(style::code_font(opts))
                .child(SharedString::from(code_block_language_label(language))),
        )
        .child(render_code_actions(
            language,
            code,
            tokens,
            opts,
            code_actions,
        ))
        .into_any_element()
}

fn render_code_actions(
    language: Option<&str>,
    code: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    let mut actions = div().flex().flex_row().items_center().gap(px(10.0));

    if is_shell_language(language)
        && let Some(on_run) = code_actions.and_then(|actions| actions.on_run.clone())
    {
        actions = actions.child(render_code_run_action(code, tokens, opts, on_run));
    }

    actions
        .child(render_code_copy_action(code, tokens, opts))
        .into_any_element()
}

fn render_code_run_action(
    code: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    on_run: MarkdownCodeRunHandler,
) -> AnyElement {
    let code = code.to_string();
    let hover_color = style::accent_color(tokens);

    render_code_action_label("RUN", tokens, opts, hover_color)
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            // Tauri emits ai-insert-command; the caller maps that to the active terminal surface.
            on_run(code.clone(), window, cx);
            cx.stop_propagation();
        })
        .into_any_element()
}

fn render_code_copy_action(code: &str, tokens: &ThemeTokens, opts: &MarkdownOptions) -> AnyElement {
    let code = code.to_string();
    let hover_color = style::text_color(tokens);

    render_code_action_label("COPY", tokens, opts, hover_color)
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            // Keep COPY local to the markdown renderer; command insertion belongs to the AI workspace.
            cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
            cx.stop_propagation();
        })
        .into_any_element()
}

fn render_code_action_label(
    label: impl Into<SharedString>,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    hover_color: Hsla,
) -> gpui::Div {
    let label = label.into();
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .py(px(2.0))
        .cursor_pointer()
        .text_size(style::code_label_font_size(opts))
        .text_color(style::code_action_color(tokens))
        .font(Font {
            weight: FontWeight::BOLD,
            ..style::code_font(opts)
        })
        .hover(move |style| style.text_color(hover_color))
        .child(label)
}

fn code_block_language_label(language: Option<&str>) -> String {
    let label = language
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or("text");
    label.to_ascii_uppercase()
}

fn is_shell_language(language: Option<&str>) -> bool {
    let normalized = language
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or("text")
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "bash" | "sh" | "zsh" | "shell" | "console" | "terminal" | "powershell" | "ps1" | "cmd"
    )
}

fn should_render_mermaid_block(language: Option<&str>, code: &str) -> bool {
    if mermaid::is_mermaid_language(language) {
        return true;
    }
    if !is_plain_text_code_language(language) {
        return false;
    }
    mermaid::is_mermaid_source_candidate(code)
}

fn is_plain_text_code_language(language: Option<&str>) -> bool {
    match language.map(str::trim).filter(|label| !label.is_empty()) {
        None => true,
        Some(label) => label.eq_ignore_ascii_case("text"),
    }
}

// ─── blockquote ─────────────────────────────────────────────────────────

fn render_blockquote_with_code_actions(
    kind: Option<CalloutKind>,
    blocks: &[Block],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    let accent = callout_color(kind, tokens);
    div()
        .flex()
        .flex_row()
        .bg(style::code_bg_color(tokens, opts))
        .border_l(px(opts.blockquote_border_width))
        .border_color(accent)
        .rounded(px(tokens.radii.sm))
        .child(
            div()
                .flex_1()
                .pl(px(opts.list_indent))
                .when_some(kind, |content, kind| {
                    content.child(render_callout_label(kind, accent, tokens, opts))
                })
                .child(render_blocks_with_code_actions(
                    blocks,
                    tokens,
                    opts,
                    code_actions,
                )),
        )
        .into_any_element()
}

fn render_selectable_blockquote(
    kind: Option<CalloutKind>,
    blocks: &[Block],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
    path: &str,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    let accent = callout_color(kind, tokens);
    div()
        .flex()
        .flex_row()
        .bg(style::code_bg_color(tokens, opts))
        .border_l(px(opts.blockquote_border_width))
        .border_color(accent)
        .rounded(px(tokens.radii.sm))
        .child(
            div()
                .flex_1()
                .pl(px(opts.list_indent))
                .when_some(kind, |content, kind| {
                    content.child(render_callout_label(kind, accent, tokens, opts))
                })
                .child(render_selectable_blocks(
                    blocks,
                    tokens,
                    opts,
                    code_actions,
                    &format!("{path}:quote"),
                    render_text,
                )),
        )
        .into_any_element()
}

fn render_callout_label(
    kind: CalloutKind,
    accent: Hsla,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    div()
        .mb(px(opts.block_gap * 0.5))
        .text_size(style::code_label_font_size(opts))
        .text_color(accent)
        .font(Font {
            weight: FontWeight::BOLD,
            ..style::body_font(opts)
        })
        .child(SharedString::from(callout_label(kind, tokens)))
        .into_any_element()
}

fn callout_label(kind: CalloutKind, _tokens: &ThemeTokens) -> &'static str {
    match kind {
        CalloutKind::Note => "NOTE",
        CalloutKind::Tip => "TIP",
        CalloutKind::Important => "IMPORTANT",
        CalloutKind::Warning => "WARNING",
        CalloutKind::Caution => "CAUTION",
    }
}

fn callout_color(kind: Option<CalloutKind>, tokens: &ThemeTokens) -> Hsla {
    match kind {
        Some(CalloutKind::Tip) => style::hex_to_hsla(tokens.ui.success),
        Some(CalloutKind::Warning) | Some(CalloutKind::Caution) => {
            style::hex_to_hsla(tokens.ui.warning)
        }
        Some(CalloutKind::Important) => style::hex_to_hsla(tokens.ui.error),
        Some(CalloutKind::Note) => style::accent_color(tokens),
        None => style::blockquote_border_color(tokens),
    }
}

// ─── table ──────────────────────────────────────────────────────────────

fn render_table(
    headers: &[Vec<Block>],
    alignments: &[TableAlignment],
    rows: &[Vec<Vec<Block>>],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    render_selectable_table(
        headers,
        alignments,
        rows,
        tokens,
        opts,
        "table",
        &mut plain_text_element,
    )
}

fn plain_text_element(
    key: MarkdownTextFragmentId,
    text: SharedString,
    runs: Vec<TextRun>,
    links: Vec<MarkdownTextLink>,
) -> AnyElement {
    let styled = StyledText::new(text).with_runs(runs);
    if links.is_empty() {
        styled.into_any_element()
    } else {
        let ranges = links.iter().map(|link| link.range.clone()).collect();
        gpui::InteractiveText::new(SharedString::from(key.key), styled)
            .on_click(ranges, move |index, window, cx| {
                (links[index].open)(window, cx)
            })
            .into_any_element()
    }
}

fn render_table_cell(
    blocks: &[Block],
    header: bool,
    path: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(opts.block_gap * 0.5))
        .children(blocks.iter().enumerate().map(|(index, block)| {
            let key = format!("{path}:{index}");
            div()
                .id(SharedString::from(key.clone()))
                .min_w_0()
                .child(match block.unlocated() {
                    Block::Paragraph { inlines } => render_selectable_inlines_with_style(
                        &key,
                        inlines,
                        tokens,
                        opts,
                        FlatRunStyle {
                            semibold: header,
                            ..Default::default()
                        },
                        render_text,
                    ),
                    _ => render_selectable_block(block, tokens, opts, None, &key, render_text),
                })
        }))
        .into_any_element()
}

fn render_selectable_table(
    headers: &[Vec<Block>],
    alignments: &[TableAlignment],
    rows: &[Vec<Vec<Block>>],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    path: &str,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    let col_count = table_column_count(headers, rows);
    let column_widths = table_column_widths(headers, rows, col_count);
    let min_width = table_min_width(headers, rows, &column_widths, tokens, opts);

    // Both rendering paths share minimum widths so selection does not alter table layout.
    let has_body_rows = !rows.is_empty();
    let header_row = div()
        .id("table-header")
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_row()
        .overflow_hidden()
        .bg(style::table_header_bg(tokens))
        .border_b_1()
        .border_color(style::table_border_color(tokens))
        // Selectable table rows need their own corner radii for the same
        // reason as normal tables: selection text is not the clipping owner.
        .rounded_t(px(tokens.radii.sm))
        .when(!has_body_rows, |row| row.rounded_b(px(tokens.radii.sm)))
        .children((0..col_count).map(|ci| {
            let cell: &[Block] = headers.get(ci).map(|v| v.as_slice()).unwrap_or(&[]);
            div()
                .id(("table-cell", ci))
                .w(relative(column_widths[ci]))
                .flex_shrink_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .whitespace_normal()
                .px(px(10.0))
                .py(px(5.0))
                .text_align(table_alignment_text_align(alignment_for_column(
                    alignments, ci,
                )))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(style::heading_color(tokens))
                .child(render_table_cell(
                    cell,
                    true,
                    &format!("{path}:th:{ci}"),
                    tokens,
                    opts,
                    render_text,
                ))
        }));

    let header_row = header_row.into_any_element();
    let header_row = if let (Some(sync), Some(span)) = (
        &opts.scroll_sync,
        headers
            .first()
            .and_then(|cell| cell.first())
            .and_then(Block::source_span),
    ) {
        sync.wrap(span, header_row)
    } else {
        header_row
    };
    let body_rows = rows.iter().enumerate().map(|(ri, row)| {
        let element = div()
            .id(("table-row", ri))
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .overflow_hidden()
            .when(ri + 1 != rows.len(), |row| {
                row.border_b_1()
                    .border_color(style::table_border_color(tokens))
            })
            .when(ri + 1 == rows.len(), |row| {
                row.rounded_b(px(tokens.radii.sm))
            })
            .children((0..col_count).map(|ci| {
                let cell: &[Block] = row.get(ci).map(|v| v.as_slice()).unwrap_or(&[]);
                div()
                    .id(("table-cell", ci))
                    .w(relative(column_widths[ci]))
                    .flex_shrink_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .whitespace_normal()
                    .px(px(10.0))
                    .py(px(5.0))
                    .text_align(table_alignment_text_align(alignment_for_column(
                        alignments, ci,
                    )))
                    .text_color(style::text_color(tokens))
                    .child(render_table_cell(
                        cell,
                        false,
                        &format!("{path}:td:{ri}:{ci}"),
                        tokens,
                        opts,
                        render_text,
                    ))
            }))
            .into_any_element();
        if let (Some(sync), Some(span)) = (
            &opts.scroll_sync,
            row.first()
                .and_then(|cell| cell.first())
                .and_then(Block::source_span),
        ) {
            sync.wrap(span, element)
        } else {
            element
        }
    });

    div()
        .id(SharedString::from(format!("markdown-table:{path}")))
        .text_size(style::body_font_size(opts))
        .font(style::body_font(opts))
        .line_height(relative(style::BODY_LINE_HEIGHT))
        .w_full()
        .min_w_0()
        .restrict_scroll_to_axis()
        .overflow_x_scrollbar()
        .child(
            div()
                .w_full()
                .min_w(px(min_width))
                .border_1()
                .border_color(style::table_border_color(tokens))
                .rounded(px(tokens.radii.sm))
                .overflow_hidden()
                .child(header_row)
                .children(body_rows),
        )
        .into_any_element()
}

fn table_min_width(
    headers: &[Vec<Block>],
    rows: &[Vec<Vec<Block>>],
    fractions: &[f32],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> f32 {
    fractions
        .iter()
        .enumerate()
        .map(|(index, fraction)| {
            let width = headers
                .get(index)
                .into_iter()
                .chain(rows.iter().filter_map(|row| row.get(index)))
                .map(|cell| table_cell_text_width(cell))
                .fold(0.0_f32, f32::max);
            let cell_width =
                width.clamp(8.0, 40.0) * opts.base_font_size * 0.55 + tokens.spacing.two * 2.0;
            cell_width / fraction
        })
        .fold(0.0, f32::max)
}

fn table_column_count(headers: &[Vec<Block>], rows: &[Vec<Vec<Block>>]) -> usize {
    headers
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0))
        .max(1)
}

fn table_column_widths(
    headers: &[Vec<Block>],
    rows: &[Vec<Vec<Block>>],
    col_count: usize,
) -> Vec<f32> {
    let mut weights = vec![6.0_f32; col_count.max(1)];
    for (ci, header) in headers.iter().enumerate().take(col_count) {
        weights[ci] = weights[ci].max(table_cell_text_width(header));
    }
    for row in rows {
        for (ci, cell) in row.iter().enumerate().take(col_count) {
            weights[ci] = weights[ci].max(table_cell_text_width(cell));
        }
    }
    for weight in &mut weights {
        *weight = weight.clamp(6.0, 32.0);
    }
    let total = weights.iter().sum::<f32>().max(1.0);
    weights.into_iter().map(|weight| weight / total).collect()
}

fn table_cell_text_width(blocks: &[Block]) -> f32 {
    blocks
        .iter()
        .map(|block| match block {
            Block::Located { block, .. } => {
                table_cell_text_width(std::slice::from_ref(block.as_ref()))
            }
            Block::Heading { inlines, .. } | Block::Paragraph { inlines } => {
                inlines.iter().map(inline_text_width).sum::<usize>() as f32
            }
            Block::Html(text) | Block::CodeBlock { code: text, .. } => {
                text.lines()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(0) as f32
            }
            Block::HtmlContainer { blocks, .. }
            | Block::Blockquote { blocks, .. }
            | Block::Details { blocks, .. } => table_cell_text_width(blocks),
            Block::OrderedList { items, .. } | Block::UnorderedList { items } => items
                .iter()
                .map(|item| {
                    (item.inlines.iter().map(inline_text_width).sum::<usize>() as f32)
                        .max(table_cell_text_width(&item.children))
                })
                .fold(0.0, f32::max),
            Block::Table { headers, rows, .. } => headers
                .iter()
                .chain(rows.iter().flatten())
                .map(|cell| table_cell_text_width(cell))
                .sum(),
            Block::HorizontalRule => 1.0,
        })
        .fold(1.0, f32::max)
}

fn inline_text_width(inline: &Inline) -> usize {
    match inline {
        Inline::Text(text) | Inline::Code(text) | Inline::Html(text) => text.chars().count(),
        Inline::Bold(children)
        | Inline::Italic(children)
        | Inline::Strikethrough(children)
        | Inline::Kbd(children)
        | Inline::Subscript(children)
        | Inline::Superscript(children)
        | Inline::Underline(children)
        | Inline::Highlight(children) => children.iter().map(inline_text_width).sum(),
        Inline::Link { text, .. } => text.iter().map(inline_text_width).sum(),
        Inline::Image { alt, .. } => alt.chars().count().max(2),
        Inline::Math { latex, .. } => latex.chars().count(),
        Inline::FootnoteReference { index, .. } => index.to_string().chars().count() + 2,
        Inline::LineBreak => 1,
    }
}

fn alignment_for_column(alignments: &[TableAlignment], column: usize) -> TableAlignment {
    alignments
        .get(column)
        .copied()
        .unwrap_or(TableAlignment::None)
}

fn table_alignment_text_align(alignment: TableAlignment) -> TextAlign {
    match alignment {
        TableAlignment::None | TableAlignment::Left => TextAlign::Left,
        TableAlignment::Center => TextAlign::Center,
        TableAlignment::Right => TextAlign::Right,
    }
}

fn block_alignment_text_align(alignment: BlockAlignment) -> TextAlign {
    match alignment {
        BlockAlignment::Left => TextAlign::Left,
        BlockAlignment::Center => TextAlign::Center,
        BlockAlignment::Right => TextAlign::Right,
    }
}

// ─── lists ──────────────────────────────────────────────────────────────

fn render_unordered_list(
    items: &[ListItem],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.one))
        .pl(px(opts.list_indent))
        .children(items.iter().enumerate().map(|(index, item)| {
            div()
                .id(("markdown-list-item", index))
                .w_full()
                .min_w_0()
                .child(render_list_item("•", item, tokens, opts, code_actions))
        }))
        .into_any_element()
}

fn render_ordered_list(
    start: u64,
    items: &[ListItem],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.one))
        .pl(px(opts.list_indent))
        .children(items.iter().enumerate().map(|(i, item)| {
            let marker = format!("{}.", start + i as u64);
            div()
                .id(("markdown-list-item", i))
                .w_full()
                .min_w_0()
                .child(render_list_item(&marker, item, tokens, opts, code_actions))
        }))
        .into_any_element()
}

fn render_selectable_unordered_list(
    items: &[ListItem],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
    path: &str,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.one))
        .pl(px(opts.list_indent))
        .children(items.iter().enumerate().map(|(index, item)| {
            render_selectable_list_item(
                "•",
                item,
                tokens,
                opts,
                code_actions,
                &format!("{path}:li:{index}"),
                render_text,
            )
        }))
        .into_any_element()
}

fn render_selectable_ordered_list(
    start: u64,
    items: &[ListItem],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
    path: &str,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.one))
        .pl(px(opts.list_indent))
        .children(items.iter().enumerate().map(|(index, item)| {
            let marker = format!("{}.", start + index as u64);
            render_selectable_list_item(
                &marker,
                item,
                tokens,
                opts,
                code_actions,
                &format!("{path}:li:{index}"),
                render_text,
            )
        }))
        .into_any_element()
}

fn render_list_item(
    marker: &str,
    item: &ListItem,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
) -> AnyElement {
    // Task list checkbox overrides the bullet/number marker when enabled.
    let effective_marker = if opts.enable_task_lists {
        match item.checked {
            Some(true) => "☑",
            Some(false) => "☐",
            None => marker,
        }
    } else {
        marker
    };

    let mut col = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .text_size(style::body_font_size(opts))
        .child({
            let header = div()
                .flex()
                .flex_row()
                .gap(px(tokens.spacing.two))
                .child(
                    div()
                        .w(px(opts.list_indent))
                        .flex_none()
                        .text_color(style::muted_color(tokens))
                        .child(SharedString::from(effective_marker.to_string())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_normal()
                        .text_color(style::text_color(tokens))
                        .child(render_styled_inlines(&item.inlines, tokens, opts)),
                )
                .into_any_element();
            if let (Some(sync), Some(span)) = (&opts.scroll_sync, item.source) {
                sync.wrap(span, header)
            } else {
                header
            }
        });

    // Render nested child blocks if present.
    if !item.children.is_empty() {
        col = col.child(
            div()
                .mt(px(opts.block_gap))
                .pl(px(opts.list_indent + tokens.spacing.two))
                .child(render_blocks_with_code_actions(
                    &item.children,
                    tokens,
                    opts,
                    code_actions,
                )),
        );
    }

    col.into_any_element()
}

fn render_selectable_list_item(
    marker: &str,
    item: &ListItem,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    code_actions: Option<&MarkdownCodeBlockActions>,
    path: &str,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    let effective_marker = if opts.enable_task_lists {
        match item.checked {
            Some(true) => "☑",
            Some(false) => "☐",
            None => marker,
        }
    } else {
        marker
    };

    let mut col = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .text_size(style::body_font_size(opts))
        .child({
            let header = div()
                .flex()
                .flex_row()
                .gap(px(tokens.spacing.two))
                .child(
                    div()
                        .w(px(opts.list_indent))
                        .flex_none()
                        .text_color(style::muted_color(tokens))
                        .child(SharedString::from(effective_marker.to_string())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_normal()
                        .text_color(style::text_color(tokens))
                        .child(render_selectable_inlines(
                            path,
                            &item.inlines,
                            tokens,
                            opts,
                            render_text,
                        )),
                )
                .into_any_element();
            if let (Some(sync), Some(span)) = (&opts.scroll_sync, item.source) {
                sync.wrap(span, header)
            } else {
                header
            }
        });

    if !item.children.is_empty() {
        col = col.child(
            div()
                .flex()
                .flex_col()
                .gap(px(opts.block_gap))
                .mt(px(opts.block_gap))
                .pl(px(opts.list_indent + tokens.spacing.two))
                .children(item.children.iter().enumerate().map(|(index, block)| {
                    render_selectable_block(
                        block,
                        tokens,
                        opts,
                        code_actions,
                        &format!("{path}:child:{index}"),
                        render_text,
                    )
                })),
        );
    }

    col.into_any_element()
}

// ─── horizontal rule ────────────────────────────────────────────────────

fn render_hr(tokens: &ThemeTokens) -> AnyElement {
    div()
        .w_full()
        .h(px(1.0))
        .bg(style::divider_color(tokens))
        .my(px(tokens.spacing.two))
        .into_any_element()
}

// ─── footnotes ──────────────────────────────────────────────────────────

fn render_footnotes(
    footnotes: &[FootnoteDefinition],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(opts.block_gap * 0.75))
        .mt(px(opts.block_gap))
        .pt(px(opts.block_gap))
        .border_t_1()
        .border_color(style::divider_color(tokens))
        .children(footnotes.iter().enumerate().map(|(index, footnote)| {
            div()
                .id(("markdown-footnote", index))
                .relative()
                .child(heading_anchor(&format!("fn:{}", index + 1), opts))
                .flex()
                .flex_row()
                .items_start()
                .gap(px(tokens.spacing.two))
                .text_size(style::footnote_font_size(opts))
                .child(
                    div()
                        .min_w(px(opts.list_indent))
                        .text_color(style::accent_color(tokens))
                        .child(render_styled_inlines(
                            &[Inline::Link {
                                text: vec![Inline::Text(format!("[{}] ↩", index + 1))],
                                url: format!("#fnback:{}", index + 1),
                            }],
                            tokens,
                            opts,
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .text_color(style::muted_color(tokens))
                        .child(render_blocks(&footnote.blocks, tokens, opts)),
                )
        }))
        .into_any_element()
}

// ─── inline rich-text rendering ─────────────────────────────────────────

/// Build a `StyledText` element from a slice of inlines with per-run `TextRun` styling.
///
/// - Bold: bold font weight
/// - Italic: italic font style
/// - Inline code: code font + `inline_code_bg_color` background
/// - Links: accent color + underline
/// - Strikethrough: `StrikethroughStyle`
/// - Images: rendered via GPUI `img()` and the surrounding async image cache
/// - Normal: default body font + text color
fn render_styled_inlines(
    inlines: &[Inline],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    render_styled_inlines_with_style(inlines, tokens, opts, FlatRunStyle::default())
}

fn render_styled_inlines_with_style(
    inlines: &[Inline],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    initial_style: FlatRunStyle,
) -> AnyElement {
    render_selectable_inlines_with_style(
        "inline",
        inlines,
        tokens,
        opts,
        initial_style,
        &mut |key, text, runs, links| {
            let styled = StyledText::new(text).with_runs(runs);
            if links.is_empty() {
                styled.into_any_element()
            } else {
                let ranges = links.iter().map(|link| link.range.clone()).collect();
                gpui::InteractiveText::new(SharedString::from(key.key), styled)
                    .on_click(ranges, move |index, window, cx| {
                        (links[index].open)(window, cx)
                    })
                    .into_any_element()
            }
        },
    )
}

fn render_selectable_inlines(
    key: &str,
    inlines: &[Inline],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    render_selectable_inlines_with_style(
        key,
        inlines,
        tokens,
        opts,
        FlatRunStyle::default(),
        render_text,
    )
}

fn render_selectable_inlines_with_style(
    key: &str,
    inlines: &[Inline],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    initial_style: FlatRunStyle,
    render_text: &mut impl FnMut(
        MarkdownTextFragmentId,
        SharedString,
        Vec<TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
) -> AnyElement {
    let mut flat = Vec::new();
    collect_runs(inlines, initial_style, &mut flat);
    let anchors = flat
        .iter()
        .filter_map(|run| run.anchor.clone())
        .collect::<Vec<_>>();
    if !flat
        .iter()
        .any(|run| run.image_url.is_some() || run.math_latex.is_some())
    {
        let (text, runs, links) = flat_text(&flat, tokens, opts);
        return with_inline_anchors(
            render_text(key.to_string().into(), text, runs, links),
            anchors,
            opts,
        );
    }
    let breaks = inline_break_offsets(&flat);
    let mut logical_cursor = 0;
    let mut children = Vec::new();
    let mut text = Vec::new();
    let mut part = 0;
    let previous_hard_break = std::cell::Cell::new(false);
    let mut flush = |text: &mut Vec<FlatRun>,
                     children: &mut Vec<InlineFlowChild>,
                     logical_cursor: &mut usize| {
        if text.is_empty() {
            return;
        }
        let (value, runs, links) = flat_text(text, tokens, opts);
        let mut start = 0;
        let ends = breaks
            .range((*logical_cursor + 1)..(*logical_cursor + value.len()))
            .map(|offset| offset - *logical_cursor)
            .chain(std::iter::once(value.len()))
            .collect::<Vec<_>>();
        for end in ends {
            let segment = &value[start..end];
            let hard_break = segment.ends_with('\n');
            let visible_end = if hard_break { end - 1 } else { end };
            let fragment = MarkdownTextFragmentId {
                key: format!("{key}:text:{part}"),
                join_previous: part > 0 && !previous_hard_break.get(),
            };
            children.push(InlineFlowChild {
                element: render_text(
                    fragment,
                    SharedString::from(&value[start..visible_end]),
                    inline_runs_slice(&runs, start..visible_end),
                    links
                        .iter()
                        .filter_map(|link| {
                            let a = link.range.start.max(start);
                            let b = link.range.end.min(visible_end);
                            (a < b).then(|| MarkdownTextLink {
                                range: a - start..b - start,
                                open: link.open.clone(),
                            })
                        })
                        .collect(),
                ),
                depth: None,
                block: false,
                break_before: breaks.contains(&(*logical_cursor + start)),
                hard_break,
            });
            previous_hard_break.set(hard_break);
            start = end;
            part += 1;
        }
        *logical_cursor += value.len();
        text.clear();
    };
    for (index, run) in flat.into_iter().enumerate() {
        if let Some(url) = &run.image_url {
            flush(&mut text, &mut children, &mut logical_cursor);
            let image = render_image(url, &run.text, run.image_dimensions, opts);
            let image = if let Some(target) = run.link_url {
                let options = opts.clone();
                div()
                    .id(SharedString::from(format!("{key}:image:{index}")))
                    .cursor_pointer()
                    .on_click(move |_, window, cx| {
                        open_markdown_link(&target, &options, window, cx);
                        cx.stop_propagation();
                    })
                    .child(image)
                    .into_any_element()
            } else {
                image
            };
            children.push(InlineFlowChild {
                element: image,
                depth: Some(0.0),
                block: false,
                break_before: breaks.contains(&logical_cursor),
                hard_break: false,
            });
            logical_cursor += '\u{fffc}'.len_utf8();
        } else if let Some(latex) = &run.math_latex {
            flush(&mut text, &mut children, &mut logical_cursor);
            let depth = math::render_math_svg(latex, run.math_display, tokens, opts)
                .ok()
                .map(|image| image.baseline_depth);
            children.push(InlineFlowChild {
                element: render_math(latex, run.math_display, tokens, opts),
                depth,
                block: run.math_display,
                break_before: run.math_display || breaks.contains(&logical_cursor),
                hard_break: run.math_display,
            });
            if run.math_display {
                previous_hard_break.set(true);
            }
            logical_cursor += '\u{fffc}'.len_utf8();
        } else {
            text.push(run);
        }
    }
    flush(&mut text, &mut children, &mut logical_cursor);
    let content = InlineFlow {
        children,
        font: style::body_font(opts),
    }
    .into_any_element();
    with_inline_anchors(content, anchors, opts)
}

fn with_inline_anchors(
    content: AnyElement,
    anchors: Vec<String>,
    opts: &MarkdownOptions,
) -> AnyElement {
    if anchors.is_empty() {
        content
    } else {
        div()
            .relative()
            .children(anchors.iter().map(|id| heading_anchor(id, opts)))
            .child(content)
            .into_any_element()
    }
}

#[derive(IntoElement)]
struct InlineFlow {
    children: Vec<InlineFlowChild>,
    font: Font,
}

impl gpui::RenderOnce for InlineFlow {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line_height = window.pixel_snap(
            text_style
                .line_height
                .to_pixels(font_size.into(), window.rem_size()),
        );
        let font_id = window.text_system().resolve_font(&self.font);
        let text_depth = inline_text_baseline_depth(
            f32::from(line_height),
            f32::from(window.text_system().ascent(font_id, font_size)),
            f32::from(window.text_system().descent(font_id, font_size)),
        );
        let depth = self
            .children
            .iter()
            .filter(|child| !child.block)
            .filter_map(|child| child.depth)
            .fold(text_depth, f32::max);
        let mut groups: Vec<Vec<InlineFlowChild>> = Vec::new();
        for child in self.children {
            let starts_line = groups
                .last()
                .and_then(|group| group.last())
                .is_some_and(|child| child.hard_break);
            if groups.is_empty() || child.block || child.break_before || starts_line {
                groups.push(Vec::new());
            }
            groups.last_mut().unwrap().push(child);
        }
        // Flex cannot infer the TeX baseline from an SVG. Align its measured descent
        // with the surrounding font rather than treating the image bottom as a baseline.
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_end()
            .when(text_style.text_align == TextAlign::Center, |row| {
                row.justify_center()
            })
            .when(text_style.text_align == TextAlign::Right, |row| {
                row.justify_end()
            })
            .children(groups.into_iter().flat_map(|group| {
                let hard_break = group.last().is_some_and(|child| child.hard_break);
                let block = group.first().is_some_and(|child| child.block);
                let element = div()
                    .min_w_0()
                    .max_w_full()
                    .flex_shrink_0()
                    .flex()
                    .items_end()
                    .when(block, |row| row.w_full())
                    .children(group.into_iter().map(|child| {
                        div()
                            .min_w_0()
                            .max_w_full()
                            .when(child.block, |piece| piece.w_full())
                            .pb(px(if child.block {
                                0.0
                            } else {
                                (depth - child.depth.unwrap_or(text_depth)).max(0.0)
                            }))
                            .child(child.element)
                    }))
                    .into_any_element();
                std::iter::once(element)
                    .chain(hard_break.then(|| div().w_full().h(px(0.0)).into_any_element()))
            }))
    }
}

struct InlineFlowChild {
    element: AnyElement,
    depth: Option<f32>,
    hard_break: bool,
    block: bool,
    break_before: bool,
}

fn inline_runs_slice(runs: &[TextRun], range: Range<usize>) -> Vec<TextRun> {
    let mut cursor = 0;
    runs.iter()
        .filter_map(|run| {
            let start = cursor;
            cursor += run.len;
            let a = start.max(range.start);
            let b = cursor.min(range.end);
            (a < b).then(|| TextRun {
                len: b - a,
                ..run.clone()
            })
        })
        .collect()
}

fn inline_break_offsets(flat: &[FlatRun]) -> std::collections::BTreeSet<usize> {
    let logical_text = flat
        .iter()
        .map(|run| {
            if run.image_url.is_some() || run.math_latex.is_some() {
                "\u{fffc}"
            } else {
                run.text.as_str()
            }
        })
        .collect::<String>();
    unicode_linebreak::linebreaks(&logical_text)
        .map(|(offset, _)| offset)
        .collect()
}

fn inline_text_baseline_depth(line_height: f32, ascent: f32, descent: f32) -> f32 {
    // Native macOS font metrics have a negative descent, whereas GPUI's shaped
    // lines paint with a positive descent. Match that convention and line leading.
    (line_height - ascent + descent.abs()) / 2.0
}

fn flat_text(
    flat: &[FlatRun],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> (SharedString, Vec<TextRun>, Vec<MarkdownTextLink>) {
    let mut text = String::new();
    let mut runs = Vec::new();
    let mut links = Vec::new();
    for run in flat {
        let start = text.len();
        text.push_str(&run.text);
        if start == text.len() {
            continue;
        }
        runs.push(text_run_for_flat(run, text.len() - start, tokens, opts));
        if let Some(url) = &run.link_url {
            let url = url.clone();
            let opts = opts.clone();
            links.push(MarkdownTextLink {
                range: start..text.len(),
                open: Rc::new(move |window, cx| open_markdown_link(&url, &opts, window, cx)),
            });
        }
    }
    (text.into(), runs, links)
}

fn heading_anchor(id: &str, opts: &MarkdownOptions) -> AnyElement {
    let id = id.to_string();
    let navigation = opts.navigation.clone();
    gpui::canvas(
        move |bounds, window, cx| {
            if let Some(navigation) = navigation {
                navigation.measure(&id, f32::from(bounds.origin.y), window, cx);
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size(px(0.0))
    .into_any_element()
}

fn open_markdown_link(url: &str, opts: &MarkdownOptions, window: &mut Window, cx: &mut App) {
    if url.starts_with('#') {
        if let Some(navigation) = &opts.navigation {
            navigation.open(url, window);
        }
    } else if let Some(resolved) = resolved_link_url(url, opts) {
        cx.open_url(&resolved);
    }
}

fn resolved_link_url(url: &str, opts: &MarkdownOptions) -> Option<String> {
    if url.starts_with('#') {
        return None;
    }
    if should_open_link(url, opts) {
        return Some(url.to_string());
    }
    if url_scheme(url).is_some() || url.starts_with("//") {
        return None;
    }
    let base = url::Url::from_directory_path(opts.image_base_dir.as_ref()?).ok()?;
    let resolved = base.join(url).ok()?;
    (resolved.scheme() == "file").then(|| resolved.to_string())
}

fn render_image(
    url: &str,
    alt: &str,
    dimensions: ImageDimensions,
    opts: &MarkdownOptions,
) -> AnyElement {
    if !opts.enable_async_images {
        return SharedString::from(format!("[Image: {}]", url)).into_any_element();
    }
    if !should_load_image(url, opts) {
        return SharedString::from(format!("[Image: {}]", url)).into_any_element();
    }

    let image = if let Some(path) = image_path_from_url(url, opts) {
        img(path)
    } else {
        img(url.to_string())
    };
    let alt = SharedString::from(alt.to_owned());
    let image = image
        .with_fallback(move || alt.clone().into_any_element())
        .max_w_full()
        .when_some(dimensions.width, |image, width| match width {
            ImageLength::Pixels(width) => image.w(px(width)),
            ImageLength::Percent(width) => image.w(relative(width / 100.0)),
        })
        .when_some(dimensions.height, |image, height| image.h(px(height)));
    div()
        .max_w_full()
        .when(dimensions == ImageDimensions::default(), |wrapper| {
            wrapper.max_w(px(opts.max_image_width))
        })
        .child(image)
        .into_any_element()
}

fn image_path_from_url(url: &str, opts: &MarkdownOptions) -> Option<PathBuf> {
    if let Some(scheme) = url_scheme(url) {
        if !image_scheme_allowed(scheme, opts) {
            return None;
        }
        if scheme.eq_ignore_ascii_case("file")
            && let Some(path) = url.strip_prefix("file://")
        {
            return Some(PathBuf::from(path));
        }
        return None;
    }

    if let Some(base_dir) = opts.image_base_dir.as_ref()
        && !PathBuf::from(url).is_absolute()
    {
        Some(base_dir.join(url))
    } else {
        Some(PathBuf::from(url))
    }
}

fn should_load_image(url: &str, opts: &MarkdownOptions) -> bool {
    let Some(scheme) = url_scheme(url) else {
        return true;
    };
    image_scheme_allowed(scheme, opts)
}

fn image_scheme_allowed(scheme: &str, opts: &MarkdownOptions) -> bool {
    opts.allowed_image_schemes
        .iter()
        .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
}

fn url_scheme(url: &str) -> Option<&str> {
    let (scheme, _) = url.split_once(':')?;
    // Treat Windows drive prefixes such as `C:\foo` as local paths instead of
    // URL schemes; markdown image paths often point at local preview assets.
    if scheme.len() <= 1 {
        return None;
    }
    if scheme
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
    {
        Some(scheme)
    } else {
        None
    }
}

fn should_open_link(url: &str, opts: &MarkdownOptions) -> bool {
    if url.starts_with('#') {
        return false;
    }
    let Some(scheme) = url_scheme(url) else {
        return false;
    };
    opts.allowed_link_schemes
        .iter()
        .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
}

fn render_math(
    latex: &str,
    display: bool,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    match math::render_math_svg(latex, display, tokens, opts) {
        Ok(rendered) => {
            let formula = img(rendered.image)
                .w(px(rendered.display_width))
                .h(px(rendered.display_height))
                .flex_shrink_0();
            if display {
                div()
                    .id("markdown-display-math")
                    .w_full()
                    .min_w_0()
                    .restrict_scroll_to_axis()
                    .overflow_x_scrollbar()
                    .child(
                        div()
                            .w_full()
                            .min_w(px(rendered.display_width))
                            .flex()
                            .justify_center()
                            .py(px(opts.math_display_padding))
                            .child(formula),
                    )
                    .into_any_element()
            } else {
                formula.into_any_element()
            }
        }
        Err(error) => {
            let text = if display {
                format!("$$ {latex} $$")
            } else {
                format!("${latex}$")
            };
            div()
                .text_size(style::code_font_size(opts))
                .text_color(style::muted_color(tokens))
                .child(SharedString::from(format!("{text} ({error})")))
                .into_any_element()
        }
    }
}

/// Build a single `TextRun` from a `FlatRun` and its byte length.
fn text_run_for_flat(
    run: &FlatRun,
    len: usize,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> TextRun {
    let mut font: Font = if run.code {
        style::code_font(opts)
    } else if run.bold && run.italic {
        Font {
            weight: FontWeight::BOLD,
            style: FontStyle::Italic,
            ..style::body_font(opts)
        }
    } else if run.bold {
        style::bold_font(opts)
    } else if run.italic {
        style::italic_font(opts)
    } else {
        style::body_font(opts)
    };
    if run.semibold && !run.bold {
        font.weight = FontWeight::SEMIBOLD;
    }
    if let Some(feature_tag) = run.script_position.open_type_feature() {
        // OpenType `subs`/`sups` keeps the run inside one selectable text
        // layout while allowing fonts with native glyphs to shift the baseline.
        font.features = FontFeatures(Arc::new(vec![(feature_tag.to_string(), 1)]));
    }

    let color: Hsla = if run.link {
        style::accent_color(tokens)
    } else {
        style::text_color(tokens)
    };

    let background_color: Option<Hsla> = if run.code {
        Some(style::inline_code_bg_color(tokens, opts))
    } else if run.highlight {
        Some(style::highlight_bg_color(tokens))
    } else {
        None
    };

    let underline: Option<UnderlineStyle> = if run.link || run.underline {
        Some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(if run.link {
                style::accent_color(tokens)
            } else {
                style::text_color(tokens)
            }),
            wavy: false,
        })
    } else {
        None
    };

    let strikethrough: Option<StrikethroughStyle> = if run.strikethrough {
        Some(StrikethroughStyle {
            thickness: px(1.0),
            color: Some(style::muted_color(tokens)),
        })
    } else {
        None
    };

    TextRun {
        len,
        font,
        color,
        background_color,
        underline,
        strikethrough,
        letter_spacing: None,
    }
}

fn plain_code_run(code: &str, tokens: &ThemeTokens, opts: &MarkdownOptions) -> TextRun {
    TextRun {
        len: code.len(),
        font: style::code_font(opts),
        color: style::text_color(tokens),
        background_color: None,
        underline: None,
        strikethrough: None,
        letter_spacing: None,
    }
}

// ─── inline → FlatRun conversion ────────────────────────────────────────

struct FlatRun {
    text: String,
    bold: bool,
    semibold: bool,
    italic: bool,
    code: bool,
    link: bool,
    link_url: Option<String>,
    strikethrough: bool,
    underline: bool,
    highlight: bool,
    script_position: ScriptPosition,
    /// If set, this run represents an image and should be rendered via `img()`.
    image_url: Option<String>,
    image_dimensions: ImageDimensions,
    anchor: Option<String>,
    math_latex: Option<String>,
    math_display: bool,
}

#[derive(Clone, Copy, Default)]
enum ScriptPosition {
    #[default]
    Normal,
    Subscript,
    Superscript,
}

impl ScriptPosition {
    fn open_type_feature(self) -> Option<&'static str> {
        match self {
            Self::Normal => None,
            Self::Subscript => Some("subs"),
            Self::Superscript => Some("sups"),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct FlatRunStyle {
    bold: bool,
    semibold: bool,
    italic: bool,
    code: bool,
    link: bool,
    strikethrough: bool,
    underline: bool,
    highlight: bool,
    script_position: ScriptPosition,
}

fn collect_runs(inlines: &[Inline], run_style: FlatRunStyle, out: &mut Vec<FlatRun>) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => {
                out.push(FlatRun {
                    text: text.clone(),
                    bold: run_style.bold,
                    semibold: run_style.semibold,
                    italic: run_style.italic,
                    code: run_style.code,
                    link: run_style.link,
                    link_url: None,
                    strikethrough: run_style.strikethrough,
                    underline: run_style.underline,
                    highlight: run_style.highlight,
                    script_position: run_style.script_position,
                    image_url: None,
                    image_dimensions: Default::default(),
                    anchor: None,
                    math_latex: None,
                    math_display: false,
                });
            }
            Inline::Bold(children) => {
                collect_runs(
                    children,
                    FlatRunStyle {
                        bold: true,
                        ..run_style
                    },
                    out,
                );
            }
            Inline::Italic(children) => {
                collect_runs(
                    children,
                    FlatRunStyle {
                        italic: true,
                        ..run_style
                    },
                    out,
                );
            }
            Inline::Code(text) => {
                out.push(FlatRun {
                    text: text.clone(),
                    bold: run_style.bold,
                    semibold: run_style.semibold,
                    italic: run_style.italic,
                    code: true,
                    link: run_style.link,
                    link_url: None,
                    strikethrough: run_style.strikethrough,
                    underline: run_style.underline,
                    highlight: run_style.highlight,
                    script_position: run_style.script_position,
                    image_url: None,
                    image_dimensions: Default::default(),
                    anchor: None,
                    math_latex: None,
                    math_display: false,
                });
            }
            Inline::Link {
                text: children,
                url,
            } => {
                let start = out.len();
                collect_runs(
                    children,
                    FlatRunStyle {
                        link: true,
                        ..run_style
                    },
                    out,
                );
                for run in &mut out[start..] {
                    run.link_url = Some(url.clone());
                }
            }
            Inline::Strikethrough(children) => {
                collect_runs(
                    children,
                    FlatRunStyle {
                        strikethrough: true,
                        ..run_style
                    },
                    out,
                );
            }
            Inline::Kbd(children) => {
                collect_runs(
                    children,
                    FlatRunStyle {
                        code: true,
                        ..run_style
                    },
                    out,
                );
            }
            Inline::Subscript(children) => {
                collect_runs(
                    children,
                    FlatRunStyle {
                        script_position: ScriptPosition::Subscript,
                        ..run_style
                    },
                    out,
                );
            }
            Inline::Superscript(children) => {
                collect_runs(
                    children,
                    FlatRunStyle {
                        script_position: ScriptPosition::Superscript,
                        ..run_style
                    },
                    out,
                );
            }
            Inline::Underline(children) => {
                collect_runs(
                    children,
                    FlatRunStyle {
                        underline: true,
                        ..run_style
                    },
                    out,
                );
            }
            Inline::Highlight(children) => {
                collect_runs(
                    children,
                    FlatRunStyle {
                        highlight: true,
                        ..run_style
                    },
                    out,
                );
            }
            Inline::Image {
                alt,
                url,
                dimensions,
            } => {
                out.push(FlatRun {
                    text: format!("[{}]", alt),
                    bold: false,
                    semibold: false,
                    italic: false,
                    code: false,
                    link: false,
                    link_url: None,
                    strikethrough: false,
                    underline: false,
                    highlight: false,
                    script_position: ScriptPosition::Normal,
                    image_url: Some(url.clone()),
                    image_dimensions: *dimensions,
                    anchor: None,
                    math_latex: None,
                    math_display: false,
                });
            }
            Inline::Math { latex, display } => {
                out.push(FlatRun {
                    text: String::new(),
                    bold: false,
                    semibold: false,
                    italic: false,
                    code: false,
                    link: false,
                    link_url: None,
                    strikethrough: false,
                    underline: false,
                    highlight: false,
                    script_position: ScriptPosition::Normal,
                    image_url: None,
                    image_dimensions: Default::default(),
                    anchor: None,
                    math_latex: Some(latex.clone()),
                    math_display: *display,
                });
            }
            Inline::FootnoteReference {
                index, occurrence, ..
            } => {
                out.push(FlatRun {
                    text: format!("[{index}]"),
                    bold: run_style.bold,
                    semibold: run_style.semibold,
                    italic: run_style.italic,
                    code: run_style.code,
                    link: true,
                    link_url: Some(format!("#fn:{index}:from:{occurrence}")),
                    strikethrough: run_style.strikethrough,
                    underline: run_style.underline,
                    highlight: run_style.highlight,
                    script_position: run_style.script_position,
                    image_url: None,
                    image_dimensions: Default::default(),
                    anchor: Some(format!("fnref:{index}:{occurrence}")),
                    math_latex: None,
                    math_display: false,
                });
            }
            Inline::LineBreak => {
                out.push(FlatRun {
                    text: "\n".into(),
                    bold: run_style.bold,
                    semibold: run_style.semibold,
                    italic: run_style.italic,
                    code: run_style.code,
                    link: run_style.link,
                    link_url: None,
                    strikethrough: run_style.strikethrough,
                    underline: run_style.underline,
                    highlight: run_style.highlight,
                    script_position: run_style.script_position,
                    image_url: None,
                    image_dimensions: Default::default(),
                    anchor: None,
                    math_latex: None,
                    math_display: false,
                });
            }
            Inline::Html(html) => {
                out.push(FlatRun {
                    text: html.clone(),
                    bold: run_style.bold,
                    semibold: run_style.semibold,
                    italic: run_style.italic,
                    code: run_style.code,
                    link: run_style.link,
                    link_url: None,
                    strikethrough: run_style.strikethrough,
                    underline: run_style.underline,
                    highlight: run_style.highlight,
                    script_position: run_style.script_position,
                    image_url: None,
                    image_dimensions: Default::default(),
                    anchor: None,
                    math_latex: None,
                    math_display: false,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn mixed_flow_keeps_punctuation_with_formula_and_preserves_image_links() {
        let mut flat = Vec::new();
        collect_runs(
            &[
                Inline::Text("甲".into()),
                Inline::Math {
                    latex: "x".into(),
                    display: false,
                },
                Inline::Text("，乙".into()),
            ],
            FlatRunStyle::default(),
            &mut flat,
        );
        assert_eq!(
            inline_break_offsets(&flat).into_iter().collect::<Vec<_>>(),
            vec![3, 9, 12]
        );
        flat.clear();
        collect_runs(
            &[Inline::Link {
                url: "https://example.com".into(),
                text: vec![Inline::Image {
                    alt: "Chart".into(),
                    url: "chart.png".into(),
                    dimensions: Default::default(),
                }],
            }],
            FlatRunStyle::default(),
            &mut flat,
        );
        assert_eq!(
            (&flat[0].image_url, &flat[0].link_url),
            (
                &Some("chart.png".into()),
                &Some("https://example.com".into())
            )
        );
    }
    #[test]
    fn inline_baseline_normalizes_signed_font_descent() {
        // A 13px ascent and 3px descent leave 3px of leading on each side
        // in a 22px line, placing its alphabetic baseline at y = 16px.
        for (line_height, descent, expected) in
            [(22.0, -3.0, 6.0), (22.0, 3.0, 6.0), (16.0, -3.0, 3.0)]
        {
            assert_eq!(
                super::inline_text_baseline_depth(line_height, 13.0, descent),
                expected
            );
        }
    }

    #[test]
    fn measured_formula_blocks_allow_scrolling_to_the_final_block() {
        let opts = super::MarkdownOptions::default();
        let document =
            crate::parser::parse(&format!("{}The end", "$$\\frac{1}{2}$$\n\n".repeat(30)));
        let measurements = crate::layout::MarkdownMeasurements::default();
        let layout = measurements.prepare(
            crate::MarkdownBlockLayout::from_document(&document, &opts),
            640.0,
            &opts,
        );
        for index in 0..30 {
            layout.record_height(index, 80.0);
        }
        layout.record_height(30, 22.0);
        let layout = measurements.prepare(
            crate::MarkdownBlockLayout::from_document(&document, &opts),
            640.0,
            &opts,
        );
        let sizes = layout.item_sizes();
        assert_eq!(
            super::estimated_markdown_height(&sizes, opts.block_gap),
            2902.0
        );
        for viewport_top in [2702.0, 2802.0, 3702.0] {
            let window =
                super::markdown_virtual_window(&sizes, opts.block_gap, viewport_top, 200.0, 0.0)
                    .unwrap();
            assert_eq!(window.range, 28..31);
            assert_eq!(window.top_spacer, 2688.0, "scroll top {viewport_top}");
            assert_eq!(window.bottom_spacer, 0.0);
        }
        let window =
            super::markdown_virtual_window(&sizes, opts.block_gap, 2602.0, 200.0, 0.0).unwrap();
        assert_eq!(window.range, 27..30);
        assert_eq!(window.top_spacer, 2592.0);
        assert_eq!(window.bottom_spacer, 38.0);
        let resized = measurements.prepare(
            crate::MarkdownBlockLayout::from_document(&document, &opts),
            320.0,
            &opts,
        );
        assert_eq!(f32::from(resized.item_sizes()[0].height), 22.0);
        let changed = crate::parser::parse("Replacement paragraph");
        let edited = measurements.prepare(
            crate::MarkdownBlockLayout::from_document(&changed, &opts),
            320.0,
            &opts,
        );
        assert_eq!(
            edited.items().as_ref(),
            &vec![crate::MarkdownLayoutItem::Block(
                crate::model::Block::Paragraph {
                    inlines: vec![crate::model::Inline::Text("Replacement paragraph".into())],
                }
            )]
        );
    }

    #[test]
    fn heading_weight_preserves_strong_emphasis() {
        let tokens = oxideterm_theme::default_tokens();
        let opts = super::MarkdownOptions::from_theme(&tokens);
        let mut flat = Vec::new();
        super::collect_runs(
            &[
                super::Inline::Text("Heading ".into()),
                super::Inline::Bold(vec![super::Inline::Text("strong".into())]),
            ],
            super::FlatRunStyle {
                semibold: true,
                ..Default::default()
            },
            &mut flat,
        );
        let (text, runs, _) = super::flat_text(&flat, &tokens, &opts);
        assert_eq!(text.as_ref(), "Heading strong");
        assert_eq!(
            runs.iter().map(|run| run.font.weight).collect::<Vec<_>>(),
            vec![gpui::FontWeight::SEMIBOLD, gpui::FontWeight::BOLD]
        );
    }

    #[test]
    fn links_share_the_paragraph_text_and_keep_byte_ranges() {
        let tokens = oxideterm_theme::default_tokens();
        let opts = super::MarkdownOptions::default();
        let mut flat = Vec::new();
        super::collect_runs(
            &[
                super::Inline::Text("中文 ".into()),
                super::Inline::Link {
                    text: vec![super::Inline::Bold(vec![super::Inline::Text(
                        "long link".into(),
                    )])],
                    url: "https://example.com".into(),
                },
                super::Inline::Text(" 后文".into()),
            ],
            super::FlatRunStyle::default(),
            &mut flat,
        );
        let (text, runs, links) = super::flat_text(&flat, &tokens, &opts);
        assert_eq!(text.as_ref(), "中文 long link 后文");
        assert_eq!(
            links
                .iter()
                .map(|link| link.range.clone())
                .collect::<Vec<_>>(),
            vec![7..16]
        );
        assert_eq!(runs[1].font.weight, gpui::FontWeight::BOLD);
    }

    #[test]
    fn relative_links_resolve_against_the_source_directory() {
        let opts = super::MarkdownOptions::default().with_source_path("/tmp/notes/readme.md");
        assert_eq!(
            super::resolved_link_url("../guide%20one.md#intro", &opts).as_deref(),
            Some("file:///tmp/guide%20one.md#intro")
        );
        assert_eq!(super::resolved_link_url("javascript:alert(1)", &opts), None);
        assert_eq!(super::resolved_link_url("//other-host/file", &opts), None);
        assert_eq!(
            super::resolved_link_url("relative.md", &super::MarkdownOptions::default()),
            None
        );
    }

    #[test]
    fn wide_tables_keep_readable_columns() {
        let tokens = oxideterm_theme::default_tokens();
        let opts = super::MarkdownOptions::default();
        let headers = vec![
            vec![Block::Paragraph {
                inlines: vec![Inline::Text("Host".into())]
            }];
            8
        ];
        let rows = vec![vec![
            vec![Block::Paragraph {
                inlines: vec![Inline::Code("command".repeat(20))]
            }];
            8
        ]];
        let fractions = super::table_column_widths(&headers, &rows, 8);
        let minimum = super::table_min_width(&headers, &rows, &fractions, &tokens, &opts);
        assert!(minimum > 1200.0);
        assert!(fractions.iter().all(|fraction| minimum * fraction >= 150.0));
    }

    use super::*;

    #[test]
    fn image_path_resolution_uses_local_paths_and_defers_remote_uris() {
        let opts = MarkdownOptions::default();
        for (url, expected) in [
            ("images/logo.png", Some("images/logo.png")),
            ("file:///tmp/logo.png", Some("/tmp/logo.png")),
            ("https://example.com/logo.png", None),
            ("http://example.com/logo.png", None),
            ("data:image/png;base64,AAAA", None),
        ] {
            assert_eq!(
                image_path_from_url(url, &opts),
                expected.map(PathBuf::from),
                "{url}"
            );
        }

        let source_opts = MarkdownOptions::default().with_source_path("/tmp/docs/README.md");
        assert_eq!(
            image_path_from_url("./assets/logo.png", &source_opts),
            Some(PathBuf::from("/tmp/docs/./assets/logo.png")),
        );
    }

    #[test]
    fn blocks_images_with_unconfigured_schemes() {
        let opts = MarkdownOptions::default();
        assert!(!should_load_image("javascript:alert(1)", &opts));
        assert!(!should_load_image("ftp://example.com/logo.png", &opts));
        assert!(should_load_image("https://example.com/logo.png", &opts));
        assert!(should_load_image("./assets/logo.png", &opts));
        assert_eq!(
            image_path_from_url("ftp://example.com/logo.png", &opts),
            None
        );
    }

    #[test]
    fn detects_mermaid_for_plain_text_code_blocks_only() {
        assert!(should_render_mermaid_block(None, "graph TD\nA --> B"));
        assert!(should_render_mermaid_block(
            Some("text"),
            "sequenceDiagram\nA->B: hi"
        ));
        assert!(should_render_mermaid_block(Some("mermaid"), "graph TD\nA"));
        assert!(!should_render_mermaid_block(
            Some("rust"),
            "graph TD\nA --> B"
        ));
        assert!(!should_render_mermaid_block(None, "echo graph TD"));
    }

    #[test]
    fn unlabeled_code_blocks_are_not_shell_runnable() {
        assert!(!is_shell_language(None));
        assert!(!is_shell_language(Some("text")));
        assert!(is_shell_language(Some("bash")));
        assert!(is_shell_language(Some("zsh")));
    }

    #[test]
    fn flat_runs_preserve_link_targets_for_click_rendering() {
        let mut runs = Vec::new();
        collect_runs(
            &[Inline::Link {
                text: vec![Inline::Text("docs".into())],
                url: "https://example.com/docs".into(),
            }],
            FlatRunStyle::default(),
            &mut runs,
        );

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "docs");
        assert!(runs[0].link);
        assert_eq!(
            runs[0].link_url.as_deref(),
            Some("https://example.com/docs")
        );
    }

    #[test]
    fn flat_runs_preserve_html_as_plain_text() {
        let mut runs = Vec::new();
        collect_runs(
            &[Inline::Html("<kbd>Esc</kbd>".into())],
            FlatRunStyle::default(),
            &mut runs,
        );

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "<kbd>Esc</kbd>");
        assert!(runs[0].link_url.is_none());
        assert!(runs[0].image_url.is_none());
        assert!(runs[0].math_latex.is_none());
    }

    #[test]
    fn flat_runs_preserve_html_underline_and_highlight_styles() {
        let mut runs = Vec::new();
        collect_runs(
            &[Inline::Underline(vec![Inline::Highlight(vec![
                Inline::Text("styled".into()),
            ])])],
            FlatRunStyle::default(),
            &mut runs,
        );

        assert_eq!(runs.len(), 1);
        assert!(runs[0].underline);
        assert!(runs[0].highlight);
        let tokens = oxideterm_theme::default_tokens();
        let opts = MarkdownOptions::from_theme(&tokens);
        let text_run = text_run_for_flat(&runs[0], runs[0].text.len(), &tokens, &opts);
        assert!(text_run.underline.is_some());
        assert_eq!(
            text_run.background_color,
            Some(style::highlight_bg_color(&tokens))
        );
    }

    #[test]
    fn flat_runs_apply_native_subscript_and_superscript_features() {
        let mut runs = Vec::new();
        collect_runs(
            &[
                Inline::Subscript(vec![Inline::Text("2".into())]),
                Inline::Superscript(vec![Inline::Text("3".into())]),
            ],
            FlatRunStyle::default(),
            &mut runs,
        );

        let tokens = oxideterm_theme::default_tokens();
        let opts = MarkdownOptions::from_theme(&tokens);
        let subscript = text_run_for_flat(&runs[0], 1, &tokens, &opts);
        let superscript = text_run_for_flat(&runs[1], 1, &tokens, &opts);
        assert_eq!(
            subscript.font.features.tag_value_list(),
            &[("subs".to_string(), 1)]
        );
        assert_eq!(
            superscript.font.features.tag_value_list(),
            &[("sups".to_string(), 1)]
        );
    }

    #[test]
    fn allows_only_configured_link_schemes() {
        let opts = MarkdownOptions::default();
        assert!(should_open_link("https://example.com", &opts));
        assert!(should_open_link("mailto:hello@example.com", &opts));
        assert!(!should_open_link("#intro", &opts));
        assert!(!should_open_link("javascript:alert(1)", &opts));
    }
}
