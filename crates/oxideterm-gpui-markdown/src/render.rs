// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Render a [`MarkdownDocument`] into a GPUI element tree.
//!
//! This module converts OxideTerm-owned markdown model nodes into composed
//! GPUI `Div` / `AnyElement` trees using only semantic theme tokens.

use std::path::PathBuf;

use gpui::{
    AnyElement, ElementId, Entity, Font, FontStyle, FontWeight, Hsla, IntoElement, ParentElement,
    Render, SharedString, StrikethroughStyle, Styled, StyledText, TextRun, UnderlineStyle, div,
    image_cache, img, px, relative, retain_all,
};
use gpui_component::{VirtualListScrollHandle, v_virtual_list};
use oxideterm_theme::ThemeTokens;

use crate::highlight;
use crate::layout::{MarkdownBlockLayout, MarkdownLayoutItem};
use crate::math;
use crate::model::{Block, FootnoteDefinition, Inline, ListItem, MarkdownDocument};
use crate::options::MarkdownOptions;
use crate::style;

const WINDOWED_MARKDOWN_MIN_ITEMS: usize = 24;

/// Render a complete markdown document into a vertical GPUI container.
pub fn render_document(
    document: &MarkdownDocument,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    let mut content = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(opts.block_gap))
        .child(render_blocks(&document.blocks, tokens, opts));

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
    let items = layout.items();
    if items.len() < WINDOWED_MARKDOWN_MIN_ITEMS || viewport_height <= 0.0 {
        return render_document(document, tokens, opts);
    }

    let item_sizes = layout.item_sizes();
    let total_height = estimated_markdown_height(&item_sizes, opts.block_gap);
    if total_height <= viewport_height + overdraw * 2.0 {
        return render_document(document, tokens, opts);
    }

    let window_top = (viewport_top - overdraw).max(0.0);
    let window_bottom = (viewport_top + viewport_height + overdraw).min(total_height);
    let mut cursor_y = 0.0;
    let mut top_spacer = 0.0;
    let mut bottom_spacer = 0.0;
    let mut rendered = Vec::new();

    for (index, item) in items.iter().enumerate() {
        let item_height = item_sizes
            .get(index)
            .map(|size| f32::from(size.height))
            .unwrap_or_default();
        let item_top = cursor_y;
        let item_bottom = item_top + item_height;
        if item_bottom >= window_top && item_top <= window_bottom {
            if rendered.is_empty() {
                top_spacer = item_top;
            }
            match item {
                MarkdownLayoutItem::Block(block) => {
                    rendered.push(render_block(block, tokens, opts));
                }
                MarkdownLayoutItem::Footnotes(footnotes) => {
                    rendered.push(render_footnotes(footnotes, tokens, opts));
                }
            }
            bottom_spacer = (total_height - item_bottom).max(0.0);
        }
        cursor_y = item_bottom;
        if index + 1 < items.len() {
            cursor_y += opts.block_gap;
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
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(opts.block_gap));
    if top_spacer > 0.0 {
        content = content.child(div().w_full().h(px((top_spacer - opts.block_gap).max(0.0))));
    }
    content = content.children(rendered);
    if bottom_spacer > 0.0 {
        content = content.child(
            div()
                .w_full()
                .h(px((bottom_spacer - opts.block_gap).max(0.0))),
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
pub fn render_document_virtual<V>(
    view: Entity<V>,
    id: impl Into<ElementId>,
    document: &MarkdownDocument,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    scroll_handle: &VirtualListScrollHandle,
) -> AnyElement
where
    V: Render,
{
    let layout = MarkdownBlockLayout::from_document(document, opts);
    let items = layout.items();
    let item_sizes = layout.item_sizes();
    let tokens = *tokens;
    let opts = opts.clone();
    let block_gap = opts.block_gap;
    let enable_async_images = opts.enable_async_images;
    let image_cache_id = opts.image_cache_id;

    let content = v_virtual_list(view, id, item_sizes, move |_this, range, _window, _cx| {
        range
            .filter_map(|index| match items.get(index) {
                Some(MarkdownLayoutItem::Block(block)) => Some(render_block(block, &tokens, &opts)),
                Some(MarkdownLayoutItem::Footnotes(footnotes)) => {
                    Some(render_footnotes(footnotes, &tokens, &opts))
                }
                None => None,
            })
            .collect::<Vec<AnyElement>>()
    })
    .gap(px(block_gap))
    .track_scroll(scroll_handle)
    .into_any_element();

    if enable_async_images {
        image_cache(retain_all(image_cache_id))
            .child(content)
            .into_any_element()
    } else {
        content
    }
}

fn estimated_markdown_height(item_sizes: &[gpui::Size<gpui::Pixels>], block_gap: f32) -> f32 {
    let items_height: f32 = item_sizes.iter().map(|size| f32::from(size.height)).sum();
    items_height + block_gap * item_sizes.len().saturating_sub(1) as f32
}

/// Render a list of blocks into a vertical GPUI container.
pub fn render_blocks(blocks: &[Block], tokens: &ThemeTokens, opts: &MarkdownOptions) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(opts.block_gap))
        .children(blocks.iter().map(|block| render_block(block, tokens, opts)))
        .into_any_element()
}

fn render_block(block: &Block, tokens: &ThemeTokens, opts: &MarkdownOptions) -> AnyElement {
    match block {
        Block::Heading { level, inlines } => render_heading(*level, inlines, tokens, opts),
        Block::Paragraph { inlines } => render_paragraph(inlines, tokens, opts),
        Block::CodeBlock { language, code } => {
            render_code_block(language.as_deref(), code, tokens, opts)
        }
        Block::UnorderedList { items } => render_unordered_list(items, tokens, opts),
        Block::OrderedList { start, items } => render_ordered_list(*start, items, tokens, opts),
        Block::HorizontalRule => render_hr(tokens),
        Block::Blockquote { blocks } => render_blockquote(blocks, tokens, opts),
        Block::Table {
            headers,
            alignments: _,
            rows,
        } => render_table(headers, rows, tokens, opts),
    }
}

// ─── headings ───────────────────────────────────────────────────────────

fn render_heading(
    level: u8,
    inlines: &[Inline],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    let font_size = style::heading_font_size(level, opts);
    div()
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
                .text_color(style::heading_color(tokens))
                .child(render_styled_inlines(inlines, tokens, opts)),
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

// ─── code blocks ────────────────────────────────────────────────────────

fn render_code_block(
    language: Option<&str>,
    code: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    let mut container = div()
        .w_full()
        .min_w_0()
        .bg(style::code_bg_color(tokens))
        .rounded(px(tokens.radii.sm))
        .p(px(opts.code_block_padding))
        .text_size(style::code_font_size(opts))
        .text_color(style::text_color(tokens));

    // If a language hint is present, render a small muted label at the top-right.
    if let Some(lang) = language {
        container = container.child(
            div()
                .flex()
                .flex_row()
                .justify_end()
                .text_size(style::code_label_font_size(opts))
                .text_color(style::muted_color(tokens))
                .child(SharedString::from(lang.to_string())),
        );
    }

    // Attempt syntax highlighting; fall back to plain monospace text.
    let code_element: AnyElement = if let Some(lang) = language {
        if let Some(runs) = highlight::highlight_code(lang, code, opts) {
            let (text, text_runs) = highlight::highlighted_runs_to_text_runs(&runs);
            StyledText::new(text)
                .with_runs(text_runs)
                .into_any_element()
        } else {
            SharedString::from(code.to_string()).into_any_element()
        }
    } else {
        SharedString::from(code.to_string()).into_any_element()
    };

    container.child(code_element).into_any_element()
}

// ─── blockquote ─────────────────────────────────────────────────────────

fn render_blockquote(blocks: &[Block], tokens: &ThemeTokens, opts: &MarkdownOptions) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .child(
            // Left border strip
            div()
                .w(px(opts.blockquote_border_width))
                .bg(style::blockquote_border_color(tokens))
                .rounded(px(tokens.radii.sm))
                .flex_shrink_0(),
        )
        .child(
            div()
                .flex_1()
                .pl(px(opts.list_indent))
                .bg(style::code_bg_color(tokens))
                .rounded(px(tokens.radii.sm))
                .child(render_blocks(blocks, tokens, opts)),
        )
        .into_any_element()
}

// ─── table ──────────────────────────────────────────────────────────────

fn render_table(
    headers: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    let col_count = headers.len().max(1);

    let header_row = div()
        .flex()
        .flex_row()
        .bg(style::table_header_bg(tokens))
        .border_b_1()
        .border_color(style::table_border_color(tokens))
        .children(headers.iter().map(|cell| {
            div()
                .flex_1()
                .p(px(tokens.spacing.two))
                .font_weight(FontWeight::BOLD)
                .text_color(style::heading_color(tokens))
                .child(render_styled_inlines(cell, tokens, opts))
        }));

    let body_rows = rows.iter().map(|row| {
        div()
            .flex()
            .flex_row()
            .border_b_1()
            .border_color(style::table_border_color(tokens))
            .children((0..col_count).map(|ci| {
                let cell: &[Inline] = row.get(ci).map(|v| v.as_slice()).unwrap_or(&[]);
                div()
                    .flex_1()
                    .p(px(tokens.spacing.two))
                    .text_color(style::text_color(tokens))
                    .child(render_styled_inlines(cell, tokens, opts))
            }))
    });

    div()
        .w_full()
        .border_1()
        .border_color(style::table_border_color(tokens))
        .rounded(px(tokens.radii.sm))
        .child(header_row)
        .children(body_rows)
        .into_any_element()
}

// ─── lists ──────────────────────────────────────────────────────────────

fn render_unordered_list(
    items: &[ListItem],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.one))
        .pl(px(opts.list_indent))
        .children(
            items
                .iter()
                .map(|item| render_list_item("•", item, tokens, opts)),
        )
        .into_any_element()
}

fn render_ordered_list(
    start: u64,
    items: &[ListItem],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
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
            render_list_item(&marker, item, tokens, opts)
        }))
        .into_any_element()
}

fn render_list_item(
    marker: &str,
    item: &ListItem,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
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
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(tokens.spacing.two))
                .child(
                    div()
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
                ),
        );

    // Render nested child blocks if present.
    if !item.children.is_empty() {
        col = col.child(
            div().flex().flex_col().mt(px(tokens.spacing.one)).children(
                item.children
                    .iter()
                    .map(|block| render_block(block, tokens, opts)),
            ),
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
                .flex()
                .flex_row()
                .items_start()
                .gap(px(tokens.spacing.two))
                .text_size(style::footnote_font_size(opts))
                .child(
                    div()
                        .min_w(px(opts.list_indent))
                        .text_color(style::accent_color(tokens))
                        .child(SharedString::from(format!("[{}]", index + 1))),
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
    let mut flat: Vec<FlatRun> = Vec::new();
    collect_runs(inlines, false, false, false, false, false, &mut flat);

    if flat.is_empty() {
        return div().into_any_element();
    }

    // If any run contains an image, use a container with mixed children.
    let has_images_or_math = flat
        .iter()
        .any(|r| r.image_url.is_some() || r.math_latex.is_some());

    if has_images_or_math {
        return render_mixed_inlines(&flat, tokens, opts);
    }

    // Fast path: all text, no images — single StyledText.
    let mut text = String::new();
    let mut runs: Vec<TextRun> = Vec::new();

    for run in &flat {
        let start = text.len();
        text.push_str(&run.text);
        let len = text.len() - start;
        if len == 0 {
            continue;
        }
        runs.push(text_run_for_flat(run, len, tokens, opts));
    }

    StyledText::new(SharedString::from(text))
        .with_runs(runs)
        .into_any_element()
}

/// Render a sequence of flat runs that contains at least one image or formula.
/// Text segments are grouped into `StyledText` elements; images and RaTeX
/// formulas are rendered as GPUI image nodes.
fn render_mixed_inlines(
    flat: &[FlatRun],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    if flat.iter().any(|run| run.math_display) {
        return render_display_mixed_inlines(flat, tokens, opts);
    }

    let mut children: Vec<AnyElement> = Vec::new();
    let mut text_buf = String::new();
    let mut run_buf: Vec<TextRun> = Vec::new();

    let flush_text =
        |text_buf: &mut String, run_buf: &mut Vec<TextRun>, children: &mut Vec<AnyElement>| {
            if !text_buf.is_empty() {
                children.push(
                    StyledText::new(SharedString::from(std::mem::take(text_buf)))
                        .with_runs(std::mem::take(run_buf))
                        .into_any_element(),
                );
            }
        };

    for run in flat {
        if let Some(ref url) = run.image_url {
            flush_text(&mut text_buf, &mut run_buf, &mut children);
            children.push(render_image(url, opts));
        } else if let Some(ref latex) = run.math_latex {
            flush_text(&mut text_buf, &mut run_buf, &mut children);
            children.push(render_math(latex, false, tokens, opts));
        } else {
            let start = text_buf.len();
            text_buf.push_str(&run.text);
            let len = text_buf.len() - start;
            if len > 0 {
                run_buf.push(text_run_for_flat(run, len, tokens, opts));
            }
        }
    }

    flush_text(&mut text_buf, &mut run_buf, &mut children);

    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(opts.block_gap * 0.35))
        .children(children)
        .into_any_element()
}

fn render_display_mixed_inlines(
    flat: &[FlatRun],
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> AnyElement {
    let mut children: Vec<AnyElement> = Vec::new();
    let mut text_buf = String::new();
    let mut run_buf: Vec<TextRun> = Vec::new();

    let flush_text =
        |text_buf: &mut String, run_buf: &mut Vec<TextRun>, children: &mut Vec<AnyElement>| {
            if !text_buf.trim().is_empty() {
                children.push(
                    div()
                        .text_size(style::body_font_size(opts))
                        .text_color(style::text_color(tokens))
                        .child(
                            StyledText::new(SharedString::from(std::mem::take(text_buf)))
                                .with_runs(std::mem::take(run_buf)),
                        )
                        .into_any_element(),
                );
            } else {
                text_buf.clear();
                run_buf.clear();
            }
        };

    for run in flat {
        if let Some(ref latex) = run.math_latex {
            flush_text(&mut text_buf, &mut run_buf, &mut children);
            children.push(render_math(latex, run.math_display, tokens, opts));
        } else if let Some(ref url) = run.image_url {
            flush_text(&mut text_buf, &mut run_buf, &mut children);
            children.push(render_image(url, opts));
        } else {
            let start = text_buf.len();
            text_buf.push_str(&run.text);
            let len = text_buf.len() - start;
            if len > 0 {
                run_buf.push(text_run_for_flat(run, len, tokens, opts));
            }
        }
    }

    flush_text(&mut text_buf, &mut run_buf, &mut children);

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(opts.block_gap * 0.5))
        .children(children)
        .into_any_element()
}

fn render_image(url: &str, opts: &MarkdownOptions) -> AnyElement {
    if !opts.enable_async_images {
        return SharedString::from(format!("[Image: {}]", url)).into_any_element();
    }

    if let Some(path) = image_path_from_url(url) {
        img(path).max_w(px(opts.max_image_width)).into_any_element()
    } else {
        img(url.to_string())
            .max_w(px(opts.max_image_width))
            .into_any_element()
    }
}

fn image_path_from_url(url: &str) -> Option<PathBuf> {
    if let Some(path) = url.strip_prefix("file://") {
        return Some(PathBuf::from(path));
    }

    let remote =
        url.starts_with("http://") || url.starts_with("https://") || url.starts_with("data:");

    if remote {
        None
    } else {
        Some(PathBuf::from(url))
    }
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
                .max_w(relative(1.0));
            if display {
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .py(px(opts.math_display_padding))
                    .child(formula)
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
    let font: Font = if run.code {
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

    let color: Hsla = if run.link {
        style::accent_color(tokens)
    } else {
        style::text_color(tokens)
    };

    let background_color: Option<Hsla> = if run.code {
        Some(style::inline_code_bg_color(tokens))
    } else {
        None
    };

    let underline: Option<UnderlineStyle> = if run.link {
        Some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(style::accent_color(tokens)),
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
    }
}

// ─── inline → FlatRun conversion ────────────────────────────────────────

struct FlatRun {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
    link: bool,
    strikethrough: bool,
    /// If set, this run represents an image and should be rendered via `img()`.
    image_url: Option<String>,
    math_latex: Option<String>,
    math_display: bool,
}

fn collect_runs(
    inlines: &[Inline],
    bold: bool,
    italic: bool,
    code: bool,
    link: bool,
    strikethrough: bool,
    out: &mut Vec<FlatRun>,
) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => {
                out.push(FlatRun {
                    text: text.clone(),
                    bold,
                    italic,
                    code,
                    link,
                    strikethrough,
                    image_url: None,
                    math_latex: None,
                    math_display: false,
                });
            }
            Inline::Bold(children) => {
                collect_runs(children, true, italic, code, link, strikethrough, out);
            }
            Inline::Italic(children) => {
                collect_runs(children, bold, true, code, link, strikethrough, out);
            }
            Inline::Code(text) => {
                out.push(FlatRun {
                    text: text.clone(),
                    bold,
                    italic,
                    code: true,
                    link,
                    strikethrough,
                    image_url: None,
                    math_latex: None,
                    math_display: false,
                });
            }
            Inline::Link { text: children, .. } => {
                collect_runs(children, bold, italic, code, true, strikethrough, out);
            }
            Inline::Strikethrough(children) => {
                collect_runs(children, bold, italic, code, link, true, out);
            }
            Inline::Image { alt, url } => {
                out.push(FlatRun {
                    text: format!("[{}]", alt),
                    bold: false,
                    italic: false,
                    code: false,
                    link: false,
                    strikethrough: false,
                    image_url: Some(url.clone()),
                    math_latex: None,
                    math_display: false,
                });
            }
            Inline::Math { latex, display } => {
                out.push(FlatRun {
                    text: String::new(),
                    bold: false,
                    italic: false,
                    code: false,
                    link: false,
                    strikethrough: false,
                    image_url: None,
                    math_latex: Some(latex.clone()),
                    math_display: *display,
                });
            }
            Inline::FootnoteReference { index, .. } => {
                out.push(FlatRun {
                    text: format!("[{}]", index),
                    bold,
                    italic,
                    code,
                    link: true,
                    strikethrough,
                    image_url: None,
                    math_latex: None,
                    math_display: false,
                });
            }
            Inline::LineBreak => {
                out.push(FlatRun {
                    text: "\n".into(),
                    bold,
                    italic,
                    code,
                    link,
                    strikethrough,
                    image_url: None,
                    math_latex: None,
                    math_display: false,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_local_image_paths() {
        assert_eq!(
            image_path_from_url("images/logo.png"),
            Some(PathBuf::from("images/logo.png")),
        );
        assert_eq!(
            image_path_from_url("file:///tmp/logo.png"),
            Some(PathBuf::from("/tmp/logo.png")),
        );
    }

    #[test]
    fn leaves_remote_images_for_async_uri_loading() {
        assert_eq!(image_path_from_url("https://example.com/logo.png"), None);
        assert_eq!(image_path_from_url("http://example.com/logo.png"), None);
        assert_eq!(image_path_from_url("data:image/png;base64,AAAA"), None);
    }
}
