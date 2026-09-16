// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Translate `pulldown-cmark` events into [`MarkdownDocument`].

use std::collections::HashMap;

use pulldown_cmark::{BlockQuoteKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::html::{self, InlineHtmlEvent, InlineHtmlKind};
use crate::model::{
    Block, CalloutKind, FootnoteDefinition, Inline, ListItem, MarkdownDocument, SourceSpan,
    TableAlignment,
};

fn markdown_options(enable_smart_punctuation: bool) -> Options {
    let mut options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS;
    if enable_smart_punctuation {
        options.insert(Options::ENABLE_SMART_PUNCTUATION);
    }
    options
}

/// Parse a markdown string into an OxideTerm-owned [`MarkdownDocument`].
pub fn parse(source: &str) -> MarkdownDocument {
    parse_with_smart_punctuation(source, true)
}

/// Parses Markdown while honoring the renderer's smart-punctuation option.
pub fn parse_with_smart_punctuation(
    source: &str,
    enable_smart_punctuation: bool,
) -> MarkdownDocument {
    parse_events(
        Parser::new_ext(source, markdown_options(enable_smart_punctuation)),
        enable_smart_punctuation,
        false,
    )
}

pub fn parse_with_source_ranges(source: &str) -> MarkdownDocument {
    parse_events(Parser::new_ext(source, markdown_options(true)), true, true)
}

#[cfg(test)]
mod source_tests {
    use super::*;
    #[test]
    fn ranges_preserve_nested_list_code_and_table_source() {
        let source = "# 中\n\n- outer\n  - 内层\n\n```rs\nlet x = 1;\n```\n\n|h|\n|-|\n|v|\n";
        let doc = parse_with_source_ranges(source);
        let slice = |span: SourceSpan| &source[span.start..span.end];
        assert_eq!(slice(doc.blocks[0].source_span().unwrap()), "# 中\n");
        let Block::UnorderedList { items } = doc.blocks[1].unlocated() else {
            panic!("list");
        };
        assert_eq!(slice(items[0].source.unwrap()), "- outer");
        let Block::UnorderedList { items } = items[0].children[0].unlocated() else {
            panic!("nested list");
        };
        assert_eq!(slice(items[0].source.unwrap()), "- 内层");
        assert_eq!(slice(doc.blocks[2].source_span().unwrap()), "let x = 1;\n");
        let Block::Table { headers, rows, .. } = doc.blocks[3].unlocated() else {
            panic!("table");
        };
        assert_eq!(slice(headers[0][0].source_span().unwrap()), "|h|\n");
        assert_eq!(slice(rows[0][0][0].source_span().unwrap()), "|v|\n");
    }
}

fn parse_events<'input>(
    parser: Parser<'input>,
    smart_punctuation: bool,
    source_ranges: bool,
) -> MarkdownDocument {
    let mut ctx = ParseContext {
        smart_punctuation,
        source_ranges,
        ..Default::default()
    };
    let mut html_block = String::new();

    for (event, range) in parser.into_offset_iter() {
        ctx.source_span = SourceSpan {
            start: range.start,
            end: range.end,
        };
        match &event {
            Event::Start(Tag::MetadataBlock(_)) => {
                ctx.in_metadata_block = true;
                continue;
            }
            Event::End(TagEnd::MetadataBlock(_)) => {
                ctx.in_metadata_block = false;
                continue;
            }
            _ if ctx.in_metadata_block => continue,
            _ => {}
        }

        match event {
            Event::Start(Tag::HtmlBlock) => html_block.clear(),
            Event::End(TagEnd::HtmlBlock) => {
                ctx.push_html(&html_block);
                html_block.clear();
            }
            // ── block-level open ────────────────────────────────────
            Event::Start(Tag::Heading { level, id, .. }) => {
                ctx.push_inline_stack();
                ctx.heading_level = Some(heading_level_to_u8(level));
                ctx.heading_explicit_id = id.map(|id| id.trim_start_matches('#').to_string());
            }
            Event::Start(Tag::Paragraph) => {
                ctx.push_inline_stack();
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                ctx.code_source = None;
                let language = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => {
                        let lang = lang.trim().to_string();
                        if lang.is_empty() { None } else { Some(lang) }
                    }
                    pulldown_cmark::CodeBlockKind::Indented => None,
                };
                ctx.code_block_lang = language;
                ctx.code_block_buf.clear();
                ctx.in_code_block = true;
            }
            Event::Start(Tag::List(start)) => {
                ctx.list_stack.push(ListState {
                    ordered_start: start,
                    items: Vec::new(),
                });
            }
            Event::Start(Tag::Item) => {
                ctx.item_sources.push(ctx.source_span);
                ctx.push_inline_stack();
                ctx.item_children.push(Vec::new());
                ctx.item_checked.push(None);
                ctx.block_containers.push(BlockContainer::ListItem);
            }
            Event::Start(Tag::BlockQuote(kind)) => {
                ctx.block_containers.push(BlockContainer::Blockquote);
                ctx.block_stack.push(BlockquoteState {
                    kind: kind.map(convert_callout_kind),
                    blocks: Vec::new(),
                });
            }
            Event::Start(Tag::Table(alignments)) => {
                ctx.table_state = Some(TableState {
                    alignments: alignments.into_iter().map(convert_alignment).collect(),
                    headers: Vec::new(),
                    rows: Vec::new(),
                    current_row: Vec::new(),
                });
            }
            Event::Start(Tag::TableHead) => {
                ctx.table_row_span = ctx.source_span;
                // The current_row will collect header cells.
                if let Some(ref mut table) = ctx.table_state {
                    table.current_row.clear();
                }
            }
            Event::Start(Tag::TableRow) => {
                ctx.table_row_span = ctx.source_span;
                if let Some(ref mut table) = ctx.table_state {
                    table.current_row.clear();
                }
            }
            Event::Start(Tag::TableCell) => {
                ctx.push_inline_stack();
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                ctx.block_containers.push(BlockContainer::Footnote);
                ctx.footnote_stack.push(FootnoteState {
                    source_start: range.start,
                    label: label.to_string(),
                    blocks: Vec::new(),
                });
            }

            // ── inline-level open ───────────────────────────────────
            Event::Start(Tag::Emphasis) => ctx.push_inline_stack(),
            Event::Start(Tag::Strong) => ctx.push_inline_stack(),
            Event::Start(Tag::Strikethrough) => ctx.push_inline_stack(),
            Event::Start(Tag::Link { dest_url, .. }) => {
                ctx.push_inline_stack();
                ctx.link_url = Some(dest_url.to_string());
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                ctx.push_inline_stack();
                ctx.image_url = Some(dest_url.to_string());
            }

            // ── text / code / breaks ────────────────────────────────
            Event::Text(text) => {
                if !ctx.in_code_block
                    && matches!(ctx.block_containers.last(), Some(BlockContainer::ListItem))
                    && ctx.item_children.last().is_some_and(Vec::is_empty)
                    && let Some(source) = ctx.item_sources.last_mut()
                {
                    source.end = range.end;
                }
                if ctx.in_code_block {
                    let source = ctx.code_source.get_or_insert(ctx.source_span);
                    source.end = range.end;
                    ctx.code_block_buf.push_str(&text);
                } else if ctx.link_url.is_some() {
                    ctx.push_inline(Inline::Text(text.to_string()));
                } else {
                    ctx.push_text_with_autolinks(&text);
                }
            }
            Event::InlineHtml(html) => {
                ctx.push_inline_html(&html);
            }
            Event::Html(html) => {
                html_block.push_str(&html);
            }
            Event::Code(code) => {
                ctx.push_inline(Inline::Code(code.to_string()));
            }
            Event::InlineMath(latex) => {
                ctx.push_inline(Inline::Math {
                    latex: latex.to_string(),
                    display: false,
                });
            }
            Event::DisplayMath(latex) => {
                ctx.push_inline(Inline::Math {
                    latex: latex.to_string(),
                    display: true,
                });
            }
            Event::SoftBreak => {
                ctx.push_inline(Inline::Text(" ".into()));
            }
            Event::HardBreak => {
                ctx.push_inline(Inline::LineBreak);
            }
            Event::FootnoteReference(label) => {
                let label = label.to_string();
                let index = ctx.footnote_index(&label);
                let occurrence = ctx.footnote_occurrences.entry(label.clone()).or_default();
                *occurrence += 1;
                let occurrence = *occurrence;
                ctx.push_inline(Inline::FootnoteReference {
                    label,
                    index,
                    occurrence,
                });
            }

            // ── task list marker ────────────────────────────────────
            Event::TaskListMarker(checked) => {
                if let Some(last) = ctx.item_checked.last_mut() {
                    *last = Some(checked);
                }
            }

            // ── block-level close ───────────────────────────────────
            Event::End(TagEnd::Heading(_level)) => {
                let inlines = ctx.pop_inline_stack();
                let level = ctx.heading_level.take().unwrap_or(1);
                let id = ctx.heading_id_for(&inlines);
                ctx.push_block(Block::Heading { level, id, inlines });
            }
            Event::End(TagEnd::Paragraph) => {
                let inlines = ctx.pop_inline_stack();
                if !inlines.is_empty() {
                    let first_item_paragraph =
                        matches!(ctx.block_containers.last(), Some(BlockContainer::ListItem))
                            && ctx.item_children.last().is_some_and(Vec::is_empty)
                            && ctx.inline_stack.last().is_some_and(Vec::is_empty);
                    if first_item_paragraph {
                        if let Some(source) = ctx.item_sources.last_mut() {
                            source.end = range.end;
                        }
                        ctx.inline_stack.last_mut().unwrap().extend(inlines);
                    } else {
                        ctx.push_block(Block::Paragraph { inlines });
                    }
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(source) = ctx.code_source.take() {
                    ctx.source_span = source;
                }
                let code = std::mem::take(&mut ctx.code_block_buf);
                let language = ctx.code_block_lang.take();
                ctx.in_code_block = false;
                ctx.push_block(Block::CodeBlock { language, code });
            }
            Event::End(TagEnd::Item) => {
                let source = ctx.item_sources.pop();
                ctx.block_containers.pop();
                let inlines = ctx.pop_inline_stack();
                let children = ctx.item_children.pop().unwrap_or_default();
                let checked = ctx.item_checked.pop().unwrap_or(None);
                if let Some(list) = ctx.list_stack.last_mut() {
                    list.items.push(ListItem {
                        source: source.filter(|_| ctx.source_ranges),
                        inlines,
                        children,
                        checked,
                    });
                }
            }
            Event::End(TagEnd::List(_)) => {
                if let Some(list) = ctx.list_stack.pop() {
                    let block = match list.ordered_start {
                        Some(start) => Block::OrderedList {
                            start,
                            items: list.items,
                        },
                        None => Block::UnorderedList { items: list.items },
                    };
                    ctx.push_block(block);
                }
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                ctx.block_containers.pop();
                let quote = ctx.block_stack.pop().unwrap_or_default();
                ctx.push_block(Block::Blockquote {
                    kind: quote.kind,
                    blocks: quote.blocks,
                });
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(ref mut table) = ctx.table_state {
                    table.headers = std::mem::take(&mut table.current_row);
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(ref mut table) = ctx.table_state {
                    let row = std::mem::take(&mut table.current_row);
                    table.rows.push(row);
                }
            }
            Event::End(TagEnd::TableCell) => {
                let inlines = ctx.pop_inline_stack();
                if let Some(ref mut table) = ctx.table_state {
                    let block = Block::Paragraph { inlines };
                    let block = if ctx.source_ranges && table.current_row.is_empty() {
                        Block::Located {
                            span: ctx.table_row_span,
                            block: Box::new(block),
                        }
                    } else {
                        block
                    };
                    table.current_row.push(vec![block]);
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(table) = ctx.table_state.take() {
                    ctx.push_block(Block::Table {
                        headers: table.headers,
                        alignments: table.alignments,
                        rows: table.rows,
                    });
                }
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                ctx.block_containers.pop();
                if let Some(mut footnote) = ctx.footnote_stack.pop() {
                    if let Some(Block::Located { span, .. }) = footnote.blocks.first_mut() {
                        span.start = footnote.source_start;
                    }
                    ctx.footnote_definitions.push(FootnoteDefinition {
                        label: footnote.label,
                        blocks: footnote.blocks,
                    });
                }
            }

            // ── inline-level close ──────────────────────────────────
            Event::End(TagEnd::Emphasis) => {
                let inner = ctx.pop_inline_stack();
                ctx.push_inline(Inline::Italic(inner));
            }
            Event::End(TagEnd::Strong) => {
                let inner = ctx.pop_inline_stack();
                ctx.push_inline(Inline::Bold(inner));
            }
            Event::End(TagEnd::Strikethrough) => {
                let inner = ctx.pop_inline_stack();
                ctx.push_inline(Inline::Strikethrough(inner));
            }
            Event::End(TagEnd::Link) => {
                let inner = ctx.pop_inline_stack();
                let url = ctx.link_url.take().unwrap_or_default();
                ctx.push_inline(Inline::Link { text: inner, url });
            }
            Event::End(TagEnd::Image) => {
                let inner = ctx.pop_inline_stack();
                let url = ctx.image_url.take().unwrap_or_default();
                // Flatten inner inlines into a plain-text alt string.
                let alt = inlines_to_plain_text(&inner);
                ctx.push_inline(Inline::Image {
                    alt,
                    url,
                    dimensions: Default::default(),
                });
            }

            // ── standalone ──────────────────────────────────────────
            Event::Rule => ctx.push_block(Block::HorizontalRule),

            // Everything else is intentionally ignored for now.
            _ => {}
        }
    }

    while matches!(ctx.block_containers.last(), Some(BlockContainer::Details)) {
        ctx.close_details();
    }
    let footnotes = ctx.ordered_footnotes();

    MarkdownDocument {
        blocks: ctx.blocks,
        footnotes,
    }
}

// ─── internal helpers ───────────────────────────────────────────────────

#[derive(Default)]
struct ParseContext {
    source_ranges: bool,
    source_span: SourceSpan,
    table_row_span: SourceSpan,
    item_sources: Vec<SourceSpan>,
    code_source: Option<SourceSpan>,
    smart_punctuation: bool,
    details: Vec<DetailsFrame>,
    summary_html: Option<String>,
    skipped_details: usize,
    blocks: Vec<Block>,
    /// Stack of inline containers — each entry collects children for one
    /// nesting level (paragraph, heading, emphasis, strong, link, list item, …).
    inline_stack: Vec<Vec<Inline>>,
    heading_level: Option<u8>,
    heading_explicit_id: Option<String>,
    code_block_lang: Option<String>,
    code_block_buf: String,
    /// Explicit flag to track whether we are inside a code block.  Using this
    /// instead of `code_block_lang.is_some()` so that indented code blocks
    /// (language = `None`) are handled correctly.
    in_code_block: bool,
    /// Frontmatter is parsed as metadata and intentionally hidden from the
    /// rendered document instead of appearing as a horizontal rule.
    in_metadata_block: bool,
    link_url: Option<String>,
    image_url: Option<String>,
    safe_html_stack: Vec<SafeInlineHtmlFrame>,
    list_stack: Vec<ListState>,
    block_containers: Vec<BlockContainer>,
    /// One entry per open `Item`; collects nested blocks within a list item.
    item_children: Vec<Vec<Block>>,
    /// One entry per open `Item`; tracks the task-list checkbox state.
    item_checked: Vec<Option<bool>>,
    /// Stack for nested blockquotes — each entry collects the blocks that
    /// belong to one level of `>` quoting.
    block_stack: Vec<BlockquoteState>,
    /// Active table accumulator, if we are inside a `<table>`.
    table_state: Option<TableState>,
    /// Stack of currently open footnote definitions.
    footnote_stack: Vec<FootnoteState>,
    /// Footnote definitions as encountered in source order.
    footnote_definitions: Vec<FootnoteDefinition>,
    /// First-reference order used for display numbering.
    footnote_reference_order: Vec<String>,
    footnote_indices: HashMap<String, usize>,
    footnote_occurrences: HashMap<String, usize>,
    heading_ids: HashMap<String, usize>,
}

struct ListState {
    ordered_start: Option<u64>,
    items: Vec<ListItem>,
}

struct TableState {
    alignments: Vec<TableAlignment>,
    headers: Vec<Vec<Block>>,
    rows: Vec<Vec<Vec<Block>>>,
    current_row: Vec<Vec<Block>>,
}

#[derive(Default)]
struct BlockquoteState {
    kind: Option<CalloutKind>,
    blocks: Vec<Block>,
}

struct FootnoteState {
    source_start: usize,
    label: String,
    blocks: Vec<Block>,
}

struct SafeInlineHtmlFrame {
    kind: InlineHtmlKind,
    source: String,
    link_url: Option<String>,
    child_stack_depth: usize,
}

enum BlockContainer {
    ListItem,
    Blockquote,
    Footnote,
    Details,
}

struct DetailsFrame {
    source_start: usize,
    id: String,
    summary: Vec<Inline>,
    blocks: Vec<Block>,
    open: bool,
}

impl ParseContext {
    fn push_html(&mut self, source: &str) {
        let mut cursor = 0;
        let boundaries = html::disclosure_boundaries(source);
        let balance: i32 = boundaries
            .iter()
            .map(|(_, boundary)| match boundary {
                html::DisclosureBoundary::Open(_) => 1,
                html::DisclosureBoundary::Close => -1,
                _ => 0,
            })
            .sum();
        if !boundaries.is_empty() && balance == 0 && html::has_enclosing_html_container(source) {
            let mut heading_id_for =
                |inlines: &[Inline], id: Option<&str>| self.unique_heading_id(inlines, id);
            let blocks = html::parse_block_fragment(source, &mut heading_id_for);
            self.push_html_blocks(blocks);
            return;
        }
        for (range, boundary) in boundaries {
            self.push_html_fragment(&source[cursor..range.start]);
            if self.skipped_details > 0
                || (self.details.len() >= html::MAX_HTML_NESTING_DEPTH
                    && matches!(boundary, html::DisclosureBoundary::Open(_)))
            {
                match boundary {
                    html::DisclosureBoundary::Open(_) => self.skipped_details += 1,
                    html::DisclosureBoundary::Close => {
                        self.skipped_details = self.skipped_details.saturating_sub(1)
                    }
                    _ => {}
                }
                self.push_block(Block::Html(source[range.clone()].to_string()));
                cursor = range.end;
                continue;
            }
            match boundary {
                html::DisclosureBoundary::Open(open) => {
                    let id = self.unique_heading_id(&[], Some("html-details"));
                    self.details.push(DetailsFrame {
                        source_start: self.source_span.start,
                        id,
                        summary: Vec::new(),
                        blocks: Vec::new(),
                        open,
                    });
                    self.block_containers.push(BlockContainer::Details);
                }
                html::DisclosureBoundary::Close => self.close_details(),
                html::DisclosureBoundary::SummaryOpen if !self.details.is_empty() => {
                    self.summary_html = Some(String::new())
                }
                html::DisclosureBoundary::SummaryClose => {
                    if let Some(source) = self.summary_html.take()
                        && let Some(frame) = self.details.last_mut()
                    {
                        frame.summary = html::summary_inlines(&source);
                    }
                }
                _ => self.push_html_fragment(&source[range.clone()]),
            }
            cursor = range.end;
        }
        self.push_html_fragment(&source[cursor..]);
    }

    fn push_html_fragment(&mut self, source: &str) {
        if self.skipped_details > 0 {
            self.push_block(Block::Html(source.to_string()));
            return;
        }
        if let Some(summary) = &mut self.summary_html {
            summary.push_str(source);
            return;
        }
        if source.trim().is_empty() {
            return;
        }
        if matches!(self.block_containers.last(), Some(BlockContainer::Details)) {
            let document = parse_with_smart_punctuation(source, self.smart_punctuation);
            let mut blocks = document.blocks;
            for block in &mut blocks {
                self.import_block(block);
            }
            self.push_html_blocks(blocks);
            for mut footnote in document.footnotes {
                for block in &mut footnote.blocks {
                    self.import_block(block);
                }
                self.footnote_definitions.push(footnote);
            }
        } else {
            let mut heading_id_for =
                |inlines: &[Inline], id: Option<&str>| self.unique_heading_id(inlines, id);
            let blocks = html::parse_block_fragment(source, &mut heading_id_for);
            self.push_html_blocks(blocks);
        }
    }

    fn import_block(&mut self, block: &mut Block) {
        match block {
            Block::Located { block, .. } => self.import_block(block),
            Block::Heading { id, inlines, .. } => {
                *id = self.unique_heading_id(inlines, Some(id));
                self.import_inlines(inlines);
            }
            Block::Paragraph { inlines } => self.import_inlines(inlines),
            Block::Details {
                id,
                blocks,
                summary,
                ..
            } => {
                self.import_inlines(summary);
                *id = self.unique_heading_id(&[], Some("html-details"));
                for block in blocks {
                    self.import_block(block);
                }
            }
            Block::Blockquote { blocks, .. } | Block::HtmlContainer { blocks, .. } => {
                for block in blocks {
                    self.import_block(block);
                }
            }
            Block::UnorderedList { items } | Block::OrderedList { items, .. } => {
                for item in items {
                    self.import_inlines(&mut item.inlines);
                    for block in &mut item.children {
                        self.import_block(block);
                    }
                }
            }
            Block::Table { headers, rows, .. } => {
                for cell in headers.iter_mut().chain(rows.iter_mut().flatten()) {
                    for block in cell {
                        self.import_block(block);
                    }
                }
            }
            _ => {}
        }
    }

    fn push_html_blocks(&mut self, blocks: Vec<Block>) {
        if self.source_ranges && blocks.len() > 1 {
            // HTML repair can change the tree. Keep one honest source range for
            // the fragment rather than assigning its range to unrelated children.
            self.push_block(Block::HtmlContainer {
                alignment: crate::model::BlockAlignment::Left,
                blocks,
            });
        } else {
            for block in blocks {
                self.push_block(block);
            }
        }
    }

    fn close_details(&mut self) {
        if matches!(self.block_containers.last(), Some(BlockContainer::Details)) {
            self.block_containers.pop();
            if let Some(frame) = self.details.pop() {
                let source = self.source_span;
                self.source_span.start = frame.source_start;
                self.push_block(Block::Details {
                    id: frame.id,
                    summary: frame.summary,
                    blocks: frame.blocks,
                    open: frame.open,
                });
                self.source_span = source;
            }
        }
    }

    fn import_inlines(&mut self, inlines: &mut [Inline]) {
        for inline in inlines {
            match inline {
                Inline::FootnoteReference {
                    label,
                    index,
                    occurrence,
                } => {
                    *index = self.footnote_index(label);
                    let count = self.footnote_occurrences.entry(label.clone()).or_default();
                    *count += 1;
                    *occurrence = *count;
                }
                Inline::Bold(children)
                | Inline::Italic(children)
                | Inline::Strikethrough(children)
                | Inline::Kbd(children)
                | Inline::Subscript(children)
                | Inline::Superscript(children)
                | Inline::Underline(children)
                | Inline::Highlight(children)
                | Inline::Link { text: children, .. } => self.import_inlines(children),
                _ => {}
            }
        }
    }

    fn push_inline_stack(&mut self) {
        self.inline_stack.push(Vec::new());
    }

    fn pop_inline_stack(&mut self) -> Vec<Inline> {
        self.close_unclosed_inline_html_at_current_depth();
        self.pop_inline_stack_raw()
    }

    fn pop_inline_stack_raw(&mut self) -> Vec<Inline> {
        self.inline_stack.pop().unwrap_or_default()
    }

    fn push_inline(&mut self, inline: Inline) {
        if let Some(top) = self.inline_stack.last_mut() {
            top.push(inline);
        }
    }

    fn push_inline_html(&mut self, html: &str) {
        match html::parse_inline_event(html) {
            InlineHtmlEvent::Node(inline) => self.push_inline(inline),
            InlineHtmlEvent::Open(open) => {
                self.push_inline_stack();
                self.safe_html_stack.push(SafeInlineHtmlFrame {
                    kind: open.kind,
                    source: html.to_string(),
                    link_url: open.link_url,
                    child_stack_depth: self.inline_stack.len(),
                });
            }
            InlineHtmlEvent::Close(kind)
                if self.safe_html_stack.last().map(|frame| frame.kind) == Some(kind) =>
            {
                let frame = self
                    .safe_html_stack
                    .pop()
                    .expect("matching inline HTML frame must exist");
                let children = self.pop_inline_stack_raw();
                for inline in wrap_safe_inline_html(frame, children) {
                    self.push_inline(inline);
                }
            }
            InlineHtmlEvent::Close(_) | InlineHtmlEvent::Unsupported => {
                // Unsupported or malformed inline HTML remains visible as inert
                // source text; the renderer never executes or interprets it.
                self.push_inline(Inline::Html(html.to_string()));
            }
        }
    }

    fn close_unclosed_inline_html_at_current_depth(&mut self) {
        while self
            .safe_html_stack
            .last()
            .is_some_and(|frame| frame.child_stack_depth == self.inline_stack.len())
        {
            let frame = self
                .safe_html_stack
                .pop()
                .expect("checked inline HTML frame must exist");
            let children = self.pop_inline_stack_raw();
            self.push_inline(Inline::Html(frame.source));
            for child in children {
                self.push_inline(child);
            }
        }
    }

    fn push_text_with_autolinks(&mut self, text: &str) {
        for inline in autolink_text(text) {
            self.push_inline(inline);
        }
    }

    fn push_block(&mut self, block: Block) {
        let block = if self.source_ranges {
            Block::Located {
                span: self.source_span,
                block: Box::new(block),
            }
        } else {
            block
        };
        // Container order matters when lists and blockquotes are nested both ways.
        match self.block_containers.last() {
            Some(BlockContainer::ListItem) => self.item_children.last_mut().unwrap().push(block),
            Some(BlockContainer::Blockquote) => {
                self.block_stack.last_mut().unwrap().blocks.push(block)
            }
            Some(BlockContainer::Footnote) => {
                self.footnote_stack.last_mut().unwrap().blocks.push(block)
            }
            Some(BlockContainer::Details) => self.details.last_mut().unwrap().blocks.push(block),
            None => self.blocks.push(block),
        }
    }

    fn footnote_index(&mut self, label: &str) -> usize {
        if let Some(index) = self.footnote_indices.get(label) {
            return *index;
        }

        let index = self.footnote_reference_order.len() + 1;
        self.footnote_reference_order.push(label.to_string());
        self.footnote_indices.insert(label.to_string(), index);
        index
    }

    fn ordered_footnotes(&mut self) -> Vec<FootnoteDefinition> {
        let mut referenced = Vec::new();
        let mut unreferenced = Vec::new();

        for footnote in std::mem::take(&mut self.footnote_definitions) {
            if let Some(index) = self.footnote_indices.get(&footnote.label) {
                referenced.push((*index, footnote));
            } else {
                unreferenced.push(footnote);
            }
        }

        referenced.sort_by_key(|(index, _)| *index);
        referenced
            .into_iter()
            .map(|(_, footnote)| footnote)
            .chain(unreferenced)
            .collect()
    }

    fn heading_id_for(&mut self, inlines: &[Inline]) -> String {
        let explicit_id = self.heading_explicit_id.take();
        self.unique_heading_id(inlines, explicit_id.as_deref())
    }

    fn unique_heading_id(&mut self, inlines: &[Inline], explicit_id: Option<&str>) -> String {
        let base = explicit_id
            .filter(|id| !id.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| slugify_heading(&inlines_to_plain_text(inlines)));
        let base = if base.is_empty() {
            "section".to_string()
        } else {
            base
        };
        let count = self.heading_ids.entry(base.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            base
        } else {
            format!("{base}-{}", *count)
        }
    }
}

fn wrap_safe_inline_html(frame: SafeInlineHtmlFrame, children: Vec<Inline>) -> Vec<Inline> {
    let inline = match frame.kind {
        InlineHtmlKind::Bold => Inline::Bold(children),
        InlineHtmlKind::Italic => Inline::Italic(children),
        InlineHtmlKind::Strikethrough => Inline::Strikethrough(children),
        InlineHtmlKind::Underline => Inline::Underline(children),
        InlineHtmlKind::Highlight => Inline::Highlight(children),
        InlineHtmlKind::Code => Inline::Code(inlines_to_plain_text(&children)),
        InlineHtmlKind::Kbd => Inline::Kbd(children),
        InlineHtmlKind::Subscript => Inline::Subscript(children),
        InlineHtmlKind::Superscript => Inline::Superscript(children),
        InlineHtmlKind::Link => Inline::Link {
            text: children,
            url: frame.link_url.unwrap_or_default(),
        },
        InlineHtmlKind::Transparent => return children,
    };
    vec![inline]
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn convert_alignment(a: pulldown_cmark::Alignment) -> TableAlignment {
    match a {
        pulldown_cmark::Alignment::None => TableAlignment::None,
        pulldown_cmark::Alignment::Left => TableAlignment::Left,
        pulldown_cmark::Alignment::Center => TableAlignment::Center,
        pulldown_cmark::Alignment::Right => TableAlignment::Right,
    }
}

fn convert_callout_kind(kind: BlockQuoteKind) -> CalloutKind {
    match kind {
        BlockQuoteKind::Note => CalloutKind::Note,
        BlockQuoteKind::Tip => CalloutKind::Tip,
        BlockQuoteKind::Important => CalloutKind::Important,
        BlockQuoteKind::Warning => CalloutKind::Warning,
        BlockQuoteKind::Caution => CalloutKind::Caution,
    }
}

/// Recursively flatten a list of [`Inline`] nodes into a single plain-text
/// string (used for image alt text).
fn inlines_to_plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(t),
            Inline::Html(html) => out.push_str(html),
            Inline::Code(c) => out.push_str(c),
            Inline::Bold(inner)
            | Inline::Italic(inner)
            | Inline::Strikethrough(inner)
            | Inline::Kbd(inner)
            | Inline::Subscript(inner)
            | Inline::Superscript(inner)
            | Inline::Underline(inner)
            | Inline::Highlight(inner)
            | Inline::Link { text: inner, .. } => {
                out.push_str(&inlines_to_plain_text(inner));
            }
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::Math { latex, display } => {
                if *display {
                    out.push_str("$$");
                    out.push_str(latex);
                    out.push_str("$$");
                } else {
                    out.push('$');
                    out.push_str(latex);
                    out.push('$');
                }
            }
            Inline::FootnoteReference { index, .. } => {
                out.push_str(&format!("[{}]", index));
            }
            Inline::LineBreak => out.push('\n'),
        }
    }
    out
}

fn slugify_heading(text: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(ch);
            pending_dash = false;
        } else if !slug.is_empty() {
            pending_dash = true;
        }
    }
    slug
}

fn autolink_text(text: &str) -> Vec<Inline> {
    let mut inlines = Vec::new();
    let mut cursor = 0;
    while cursor < text.len() {
        let Some(relative_start) = find_url_start(&text[cursor..]) else {
            push_text_fragment(&mut inlines, &text[cursor..]);
            break;
        };
        let start = cursor + relative_start;
        push_text_fragment(&mut inlines, &text[cursor..start]);

        let mut end = start;
        for (offset, ch) in text[start..].char_indices() {
            if ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'') {
                break;
            }
            end = start + offset + ch.len_utf8();
        }
        while end > start
            && text[..end]
                .chars()
                .next_back()
                .is_some_and(|ch| matches!(ch, '.' | ',' | ';' | ':' | ')' | ']' | '}'))
        {
            let ch_len = text[..end]
                .chars()
                .next_back()
                .map(char::len_utf8)
                .unwrap_or(0);
            end = end.saturating_sub(ch_len);
        }

        if end == start {
            push_text_fragment(&mut inlines, &text[start..start + 1]);
            cursor = start + 1;
            continue;
        }

        let url = &text[start..end];
        inlines.push(Inline::Link {
            text: vec![Inline::Text(url.to_string())],
            url: url.to_string(),
        });
        cursor = end;
    }
    inlines
}

fn push_text_fragment(inlines: &mut Vec<Inline>, text: &str) {
    if !text.is_empty() {
        inlines.push(Inline::Text(text.to_string()));
    }
}

fn find_url_start(text: &str) -> Option<usize> {
    ["https://", "http://"]
        .into_iter()
        .filter_map(|needle| text.find(needle))
        .min()
}

// ─── tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[test]
    fn list_paragraphs_and_nested_blocks_keep_source_order() {
        let doc = super::parse(
            "- First\n\n  Second\n\n  > Quoted\n\n  ```sh\n  echo done\n  ```\n\n  - Child\n\n  Last\n",
        );
        let text = |value: &str| vec![super::Inline::Text(value.into())];
        assert_eq!(
            doc.blocks,
            vec![super::Block::UnorderedList {
                items: vec![super::ListItem {
                    source: None,
                    inlines: text("First"),
                    checked: None,
                    children: vec![
                        super::Block::Paragraph {
                            inlines: text("Second")
                        },
                        super::Block::Blockquote {
                            kind: None,
                            blocks: vec![super::Block::Paragraph {
                                inlines: text("Quoted")
                            }]
                        },
                        super::Block::CodeBlock {
                            language: Some("sh".into()),
                            code: "echo done\n".into()
                        },
                        super::Block::UnorderedList {
                            items: vec![super::ListItem {
                                source: None,
                                inlines: text("Child"),
                                checked: None,
                                children: vec![]
                            }]
                        },
                        super::Block::Paragraph {
                            inlines: text("Last")
                        },
                    ],
                }],
            }]
        );
    }

    use super::*;

    #[test]
    fn parses_inline_and_display_math() {
        let doc = parse("Inline $a^2+b^2=c^2$.\n\n$$\\frac{1}{2}$$");
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Paragraph { inlines } => {
                assert!(inlines.iter().any(|inline| matches!(
                    inline,
                    Inline::Math { latex, display: false } if latex == "a^2+b^2=c^2"
                )));
            }
            other => panic!("expected inline math Paragraph, got {:?}", other),
        }
        match &doc.blocks[1] {
            Block::Paragraph { inlines } => {
                assert!(inlines.iter().any(|inline| matches!(
                    inline,
                    Inline::Math { latex, display: true } if latex == "\\frac{1}{2}"
                )));
            }
            other => panic!("expected display math Paragraph, got {:?}", other),
        }
    }

    #[test]
    fn smart_punctuation_can_be_disabled_by_render_options() {
        let doc = parse_with_smart_punctuation("'quoted'", false);

        assert!(matches!(
            &doc.blocks[0],
            Block::Paragraph { inlines }
                if inlines == &vec![Inline::Text("'quoted'".to_string())]
        ));
    }

    #[test]
    fn parses_bare_http_urls_as_links() {
        let doc = parse("See https://example.com/docs.");
        match &doc.blocks[0] {
            Block::Paragraph { inlines } => {
                assert!(inlines.iter().any(|inline| matches!(
                    inline,
                    Inline::Link { text, url }
                        if url == "https://example.com/docs"
                            && text == &vec![Inline::Text("https://example.com/docs".into())]
                )));
                assert!(
                    inlines
                        .iter()
                        .any(|inline| matches!(inline, Inline::Text(text) if text == "."))
                );
            }
            other => panic!("expected Paragraph, got {:?}", other),
        }
    }

    #[test]
    fn hides_yaml_frontmatter() {
        let doc = parse("---\ntitle: Demo\n---\n\n# Body");
        assert_eq!(doc.blocks.len(), 1);
        assert!(matches!(
            &doc.blocks[0],
            Block::Heading { id, .. } if id == "body"
        ));
    }

    #[test]
    fn parses_gfm_callout_kind() {
        let doc = parse("> [!WARNING]\n> Careful");
        match &doc.blocks[0] {
            Block::Blockquote { kind, blocks } => {
                assert_eq!(*kind, Some(CalloutKind::Warning));
                assert_eq!(blocks.len(), 1);
            }
            other => panic!("expected warning callout, got {:?}", other),
        }
    }

    #[test]
    fn preserves_explicit_heading_id_and_uniques_generated_slugs() {
        let doc = parse("# Intro {#custom}\n\n# Intro\n\n# Intro");
        assert!(matches!(
            &doc.blocks[0],
            Block::Heading { id, .. } if id == "custom"
        ));
        assert!(matches!(
            &doc.blocks[1],
            Block::Heading { id, .. } if id == "intro"
        ));
        assert!(matches!(
            &doc.blocks[2],
            Block::Heading { id, .. } if id == "intro-2"
        ));
    }

    #[test]
    fn parses_footnote_reference_and_definition() {
        let doc = parse("Hello[^note].\n\n[^note]: Footnote **body**.");

        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Paragraph { inlines } => {
                assert!(inlines.iter().any(|inline| matches!(
                    inline,
                    Inline::FootnoteReference { label, index, .. }
                        if label == "note" && *index == 1
                )));
            }
            other => panic!("expected Paragraph, got {:?}", other),
        }

        assert_eq!(doc.footnotes.len(), 1);
        assert_eq!(doc.footnotes[0].label, "note");
        assert_eq!(doc.footnotes[0].blocks.len(), 1);
        match &doc.footnotes[0].blocks[0] {
            Block::Paragraph { inlines } => {
                assert!(inlines.iter().any(|inline| matches!(
                    inline,
                    Inline::Bold(children)
                        if children.iter().any(|child| matches!(child, Inline::Text(text) if text == "body"))
                )));
            }
            other => panic!("expected footnote Paragraph, got {:?}", other),
        }
    }

    #[test]
    fn orders_footnotes_by_first_reference() {
        let doc = parse("Second[^b] then first[^a].\n\n[^a]: A\n\n[^b]: B");

        assert_eq!(doc.footnotes.len(), 2);
        assert_eq!(doc.footnotes[0].label, "b");
        assert_eq!(doc.footnotes[1].label, "a");
    }

    #[test]
    fn preserves_unsupported_inline_html_as_inert_text() {
        let doc = parse("Text <custom-tag data-value=\"x\">inline</custom-tag> html");
        match &doc.blocks[0] {
            Block::Paragraph { inlines } => {
                assert!(inlines.iter().any(|inline| matches!(
                    inline,
                    Inline::Html(html) if html.contains("<custom-tag")
                )));
            }
            other => panic!("expected Paragraph, got {:?}", other),
        }
    }

    #[test]
    fn parses_safe_inline_html_subset() {
        let doc = parse("Press <kbd>Esc</kbd><br>H<sub>2</sub>O x<sup>2</sup>");
        match &doc.blocks[0] {
            Block::Paragraph { inlines } => {
                assert!(inlines.iter().any(|inline| matches!(
                    inline,
                    Inline::Kbd(children)
                        if children == &vec![Inline::Text("Esc".to_string())]
                )));
                assert!(
                    inlines
                        .iter()
                        .any(|inline| matches!(inline, Inline::LineBreak))
                );
                assert!(inlines.iter().any(|inline| matches!(
                    inline,
                    Inline::Subscript(children)
                        if children == &vec![Inline::Text("2".to_string())]
                )));
                assert!(inlines.iter().any(|inline| matches!(
                    inline,
                    Inline::Superscript(children)
                        if children == &vec![Inline::Text("2".to_string())]
                )));
            }
            other => panic!("expected Paragraph, got {:?}", other),
        }
    }

    #[test]
    fn parses_common_block_html_into_native_blocks() {
        let doc = parse("<div>raw</div>\n\nAfter");

        assert!(matches!(
            &doc.blocks[0],
            Block::Paragraph { inlines }
                if inlines == &vec![Inline::Text("raw".to_string())]
        ));
        assert!(matches!(&doc.blocks[1], Block::Paragraph { .. }));
    }

    #[test]
    fn multiline_html_preserves_table_and_list_structure() {
        let table = parse("<table>\n<tr><th>A</th></tr>\n<tr><td>1</td></tr>\n</table>");
        assert_eq!(
            table.blocks,
            vec![Block::Table {
                headers: vec![vec![Block::Paragraph {
                    inlines: vec![Inline::Text("A".into())]
                }]],
                alignments: vec![TableAlignment::None],
                rows: vec![vec![vec![Block::Paragraph {
                    inlines: vec![Inline::Text("1".into())]
                }]]],
            }]
        );
        let list =
            parse("<ul>\n<li><p>First</p><p>Second</p><pre>code</pre><p>Last</p></li>\n</ul>");
        assert_eq!(
            list.blocks,
            vec![Block::UnorderedList {
                items: vec![ListItem {
                    source: None,
                    inlines: vec![Inline::Text("First".into())],
                    children: vec![
                        Block::Paragraph {
                            inlines: vec![Inline::Text("Second".into())]
                        },
                        Block::CodeBlock {
                            language: None,
                            code: "code".into()
                        },
                        Block::Paragraph {
                            inlines: vec![Inline::Text("Last".into())]
                        },
                    ],
                    checked: None,
                }]
            }]
        );
    }

    #[test]
    fn disclosure_keeps_markdown_across_blank_lines_and_nested_disclosures() {
        let doc = parse(
            "<details>\n<summary>More</summary>\n\n## Inside\n\n**Bold**\n\n<details open><summary>Nested</summary>\n\n```rust\nlet n = 1;\n```\n\n</details>\n</details>\n\nAfter",
        );
        assert_eq!(
            doc.blocks,
            vec![
                Block::Details {
                    id: "html-details".into(),
                    summary: vec![Inline::Text("More".into())],
                    open: false,
                    blocks: vec![
                        Block::Heading {
                            level: 2,
                            id: "inside".into(),
                            inlines: vec![Inline::Text("Inside".into())]
                        },
                        Block::Paragraph {
                            inlines: vec![Inline::Bold(vec![Inline::Text("Bold".into())])]
                        },
                        Block::Details {
                            id: "html-details-2".into(),
                            summary: vec![Inline::Text("Nested".into())],
                            open: true,
                            blocks: vec![Block::CodeBlock {
                                language: Some("rust".into()),
                                code: "let n = 1;\n".into()
                            }]
                        },
                    ],
                },
                Block::Paragraph {
                    inlines: vec![Inline::Text("After".into())]
                }
            ]
        );
        let code = parse("```html\n<details><summary>Literal</summary></details>\n```");
        assert_eq!(
            code.blocks,
            vec![Block::CodeBlock {
                language: Some("html".into()),
                code: "<details><summary>Literal</summary></details>\n".into()
            }]
        );
    }

    #[test]
    fn html_table_cells_keep_paragraphs_lists_and_code() {
        let doc = parse(
            "<table><tr><td><p>A</p><p>B</p><ul><li>C</li></ul><pre>code</pre></td></tr></table>",
        );
        assert_eq!(
            doc.blocks,
            vec![Block::Table {
                headers: Vec::new(),
                alignments: vec![TableAlignment::None],
                rows: vec![vec![vec![
                    Block::Paragraph {
                        inlines: vec![Inline::Text("A".into())]
                    },
                    Block::Paragraph {
                        inlines: vec![Inline::Text("B".into())]
                    },
                    Block::UnorderedList {
                        items: vec![ListItem {
                            source: None,
                            inlines: vec![Inline::Text("C".into())],
                            children: Vec::new(),
                            checked: None
                        }]
                    },
                    Block::CodeBlock {
                        language: None,
                        code: "code".into()
                    },
                ]]]
            }]
        );
    }

    #[test]
    fn disclosure_markdown_preserves_the_surrounding_quote_and_list() {
        let doc = parse("> <details>\n> <summary>More</summary>\n>\n> - Item\n>\n> </details>");
        assert_eq!(
            doc.blocks,
            vec![Block::Blockquote {
                kind: None,
                blocks: vec![Block::Details {
                    id: "html-details".into(),
                    summary: vec![Inline::Text("More".into())],
                    open: false,
                    blocks: vec![Block::UnorderedList {
                        items: vec![ListItem {
                            source: None,
                            inlines: vec![Inline::Text("Item".into())],
                            children: Vec::new(),
                            checked: None,
                        }]
                    }],
                }]
            }]
        );
    }

    #[test]
    fn keeps_heading_ids_unique_across_markdown_and_html() {
        let doc = parse("# Intro\n\n<h1>Intro</h1>\n\n<h1 id='intro'>Explicit</h1>");

        assert!(matches!(
            &doc.blocks[0],
            Block::Heading { id, .. } if id == "intro"
        ));
        assert!(matches!(
            &doc.blocks[1],
            Block::Heading { id, .. } if id == "intro-2"
        ));
        assert!(matches!(
            &doc.blocks[2],
            Block::Heading { id, .. } if id == "intro-3"
        ));
    }

    #[test]
    fn parses_extended_inline_html_with_attributes() {
        let doc = parse(
            "<span class='ignored'><u>under</u> <mark>marked</mark> <a href='https://example.com'>link</a> <img src='https://example.com/a.png' alt='A'></span>",
        );

        let Block::Paragraph { inlines } = &doc.blocks[0] else {
            panic!("expected Paragraph, got {:?}", doc.blocks[0]);
        };
        assert!(inlines.iter().any(|inline| matches!(
            inline,
            Inline::Underline(children)
                if children == &vec![Inline::Text("under".to_string())]
        )));
        assert!(inlines.iter().any(|inline| matches!(
            inline,
            Inline::Highlight(children)
                if children == &vec![Inline::Text("marked".to_string())]
        )));
        assert!(inlines.iter().any(|inline| matches!(
            inline,
            Inline::Link { text, url }
                if text == &vec![Inline::Text("link".to_string())]
                    && url == "https://example.com"
        )));
        assert!(inlines.iter().any(|inline| matches!(
            inline,
            Inline::Image { alt, url, .. }
                if alt == "A" && url == "https://example.com/a.png"
        )));
    }

    #[test]
    fn keeps_content_when_safe_inline_html_is_unclosed() {
        let doc = parse("before <mark>after");

        let Block::Paragraph { inlines } = &doc.blocks[0] else {
            panic!("expected Paragraph, got {:?}", doc.blocks[0]);
        };
        assert!(
            inlines
                .iter()
                .any(|inline| matches!(inline, Inline::Html(source) if source == "<mark>"))
        );
        assert!(
            inlines
                .iter()
                .any(|inline| matches!(inline, Inline::Text(text) if text == "after"))
        );
    }

    #[test]
    fn parses_html_lists_tables_and_code_blocks() {
        let source = "<ol start='3'><li>three</li><li>four<ul><li>nested</li></ul></li></ol>\n\n<table><thead><tr><th align='right'>A</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>\n\n<pre><code class='language-rust'>fn main() {}</code></pre>";
        let doc = parse(source);

        assert!(matches!(
            &doc.blocks[0],
            Block::OrderedList { start: 3, items } if items.len() == 2 && !items[1].children.is_empty()
        ));
        assert!(matches!(
            &doc.blocks[1],
            Block::Table { headers, alignments, rows }
                if headers.len() == 1
                    && alignments == &vec![TableAlignment::Right]
                    && rows.len() == 1
        ));
        assert!(matches!(
            &doc.blocks[2],
            Block::CodeBlock { language, code }
                if language.as_deref() == Some("rust") && code == "fn main() {}"
        ));
    }

    #[test]
    fn drops_active_html_content_without_losing_safe_siblings() {
        let doc = parse("<div>before<script>alert(1)</script><style>body{}</style>after</div>");

        assert_eq!(
            doc.blocks,
            vec![Block::Paragraph {
                inlines: vec![
                    Inline::Text("before".to_string()),
                    Inline::Text("after".to_string()),
                ],
            }]
        );
    }
}
