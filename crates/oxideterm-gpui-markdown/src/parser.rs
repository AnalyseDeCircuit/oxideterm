// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Translate `pulldown-cmark` events into [`MarkdownDocument`].

use std::collections::HashMap;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::model::{Block, FootnoteDefinition, Inline, ListItem, MarkdownDocument, TableAlignment};

/// Parse a markdown string into an OxideTerm-owned [`MarkdownDocument`].
pub fn parse(source: &str) -> MarkdownDocument {
    let options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_SMART_PUNCTUATION;
    let parser = Parser::new_ext(source, options);

    let mut ctx = ParseContext::default();

    for event in parser {
        match event {
            // ── block-level open ────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                ctx.push_inline_stack();
                ctx.heading_level = Some(heading_level_to_u8(level));
            }
            Event::Start(Tag::Paragraph) => {
                ctx.push_inline_stack();
            }
            Event::Start(Tag::CodeBlock(kind)) => {
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
                ctx.push_inline_stack();
                ctx.item_children.push(Vec::new());
                ctx.item_checked.push(None);
            }
            Event::Start(Tag::BlockQuote(_)) => {
                ctx.block_stack.push(Vec::new());
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
                // The current_row will collect header cells.
                if let Some(ref mut table) = ctx.table_state {
                    table.current_row.clear();
                }
            }
            Event::Start(Tag::TableRow) => {
                if let Some(ref mut table) = ctx.table_state {
                    table.current_row.clear();
                }
            }
            Event::Start(Tag::TableCell) => {
                ctx.push_inline_stack();
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                ctx.footnote_stack.push(FootnoteState {
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
                if ctx.in_code_block {
                    ctx.code_block_buf.push_str(&text);
                } else {
                    ctx.push_inline(Inline::Text(text.to_string()));
                }
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
                ctx.push_inline(Inline::FootnoteReference { label, index });
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
                ctx.push_block(Block::Heading { level, inlines });
            }
            Event::End(TagEnd::Paragraph) => {
                let inlines = ctx.pop_inline_stack();
                if !inlines.is_empty() {
                    if ctx.list_stack.is_empty() {
                        ctx.push_block(Block::Paragraph { inlines });
                    } else {
                        // Paragraph inside a list item — merge inlines into the
                        // current item's inline stack instead of emitting a block.
                        if let Some(top) = ctx.inline_stack.last_mut() {
                            top.extend(inlines);
                        }
                    }
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                let code = std::mem::take(&mut ctx.code_block_buf);
                let language = ctx.code_block_lang.take();
                ctx.in_code_block = false;
                ctx.push_block(Block::CodeBlock { language, code });
            }
            Event::End(TagEnd::Item) => {
                let inlines = ctx.pop_inline_stack();
                let children = ctx.item_children.pop().unwrap_or_default();
                let checked = ctx.item_checked.pop().unwrap_or(None);
                if let Some(list) = ctx.list_stack.last_mut() {
                    list.items.push(ListItem {
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
                    // If still inside a parent list item, attach as child block.
                    if let Some(children) = ctx.item_children.last_mut() {
                        children.push(block);
                    } else {
                        ctx.push_block(block);
                    }
                }
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                let blocks = ctx.block_stack.pop().unwrap_or_default();
                ctx.push_block(Block::Blockquote { blocks });
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
                    table.current_row.push(inlines);
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
                if let Some(footnote) = ctx.footnote_stack.pop() {
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
                ctx.push_inline(Inline::Image { alt, url });
            }

            // ── standalone ──────────────────────────────────────────
            Event::Rule => ctx.push_block(Block::HorizontalRule),

            // Everything else is intentionally ignored for now.
            _ => {}
        }
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
    blocks: Vec<Block>,
    /// Stack of inline containers — each entry collects children for one
    /// nesting level (paragraph, heading, emphasis, strong, link, list item, …).
    inline_stack: Vec<Vec<Inline>>,
    heading_level: Option<u8>,
    code_block_lang: Option<String>,
    code_block_buf: String,
    /// Explicit flag to track whether we are inside a code block.  Using this
    /// instead of `code_block_lang.is_some()` so that indented code blocks
    /// (language = `None`) are handled correctly.
    in_code_block: bool,
    link_url: Option<String>,
    image_url: Option<String>,
    list_stack: Vec<ListState>,
    /// One entry per open `Item`; collects nested blocks within a list item.
    item_children: Vec<Vec<Block>>,
    /// One entry per open `Item`; tracks the task-list checkbox state.
    item_checked: Vec<Option<bool>>,
    /// Stack for nested blockquotes — each entry collects the blocks that
    /// belong to one level of `>` quoting.
    block_stack: Vec<Vec<Block>>,
    /// Active table accumulator, if we are inside a `<table>`.
    table_state: Option<TableState>,
    /// Stack of currently open footnote definitions.
    footnote_stack: Vec<FootnoteState>,
    /// Footnote definitions as encountered in source order.
    footnote_definitions: Vec<FootnoteDefinition>,
    /// First-reference order used for display numbering.
    footnote_reference_order: Vec<String>,
    footnote_indices: HashMap<String, usize>,
}

struct ListState {
    ordered_start: Option<u64>,
    items: Vec<ListItem>,
}

struct TableState {
    alignments: Vec<TableAlignment>,
    headers: Vec<Vec<Inline>>,
    rows: Vec<Vec<Vec<Inline>>>,
    current_row: Vec<Vec<Inline>>,
}

struct FootnoteState {
    label: String,
    blocks: Vec<Block>,
}

impl ParseContext {
    fn push_inline_stack(&mut self) {
        self.inline_stack.push(Vec::new());
    }

    fn pop_inline_stack(&mut self) -> Vec<Inline> {
        self.inline_stack.pop().unwrap_or_default()
    }

    fn push_inline(&mut self, inline: Inline) {
        if let Some(top) = self.inline_stack.last_mut() {
            top.push(inline);
        }
    }

    /// Push a block into the innermost open container.  If a blockquote is
    /// open the block goes there; otherwise it lands in the top-level list.
    fn push_block(&mut self, block: Block) {
        if let Some(bq) = self.block_stack.last_mut() {
            bq.push(block);
        } else if let Some(footnote) = self.footnote_stack.last_mut() {
            footnote.blocks.push(block);
        } else {
            self.blocks.push(block);
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

/// Recursively flatten a list of [`Inline`] nodes into a single plain-text
/// string (used for image alt text).
fn inlines_to_plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(t),
            Inline::Code(c) => out.push_str(c),
            Inline::Bold(inner)
            | Inline::Italic(inner)
            | Inline::Strikethrough(inner)
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

// ─── tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_heading() {
        let doc = parse("# Hello");
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Heading { level, inlines } => {
                assert_eq!(*level, 1);
                assert_eq!(inlines.len(), 1);
                assert_eq!(inlines[0], Inline::Text("Hello".into()));
            }
            other => panic!("expected Heading, got {:?}", other),
        }
    }

    #[test]
    fn parses_paragraph_with_bold_italic() {
        let doc = parse("Hello **bold** and *italic* world");
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Paragraph { inlines } => {
                assert!(inlines.len() >= 3);
                // Find bold
                assert!(inlines.iter().any(|i| matches!(i, Inline::Bold(_))));
                // Find italic
                assert!(inlines.iter().any(|i| matches!(i, Inline::Italic(_))));
            }
            other => panic!("expected Paragraph, got {:?}", other),
        }
    }

    #[test]
    fn parses_code_block() {
        let doc = parse("```rust\nfn main() {}\n```");
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::CodeBlock { language, code } => {
                assert_eq!(language.as_deref(), Some("rust"));
                assert!(code.contains("fn main()"));
            }
            other => panic!("expected CodeBlock, got {:?}", other),
        }
    }

    #[test]
    fn parses_unordered_list() {
        let doc = parse("- one\n- two\n- three");
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::UnorderedList { items } => {
                assert_eq!(items.len(), 3);
            }
            other => panic!("expected UnorderedList, got {:?}", other),
        }
    }

    #[test]
    fn parses_ordered_list() {
        let doc = parse("1. first\n2. second");
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::OrderedList { start, items } => {
                assert_eq!(*start, 1);
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected OrderedList, got {:?}", other),
        }
    }

    #[test]
    fn parses_hr() {
        let doc = parse("---");
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.blocks[0], Block::HorizontalRule);
    }

    #[test]
    fn parses_inline_code() {
        let doc = parse("Use `cargo build` here");
        match &doc.blocks[0] {
            Block::Paragraph { inlines } => {
                assert!(
                    inlines
                        .iter()
                        .any(|i| matches!(i, Inline::Code(c) if c == "cargo build"))
                );
            }
            other => panic!("expected Paragraph, got {:?}", other),
        }
    }

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
    fn parses_link() {
        let doc = parse("[click](https://example.com)");
        match &doc.blocks[0] {
            Block::Paragraph { inlines } => {
                assert!(inlines.iter().any(
                    |i| matches!(i, Inline::Link { url, .. } if url == "https://example.com")
                ));
            }
            other => panic!("expected Paragraph, got {:?}", other),
        }
    }

    // ── new tests ───────────────────────────────────────────────────────

    #[test]
    fn parses_blockquote() {
        let doc = parse("> Hello world");
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Blockquote { blocks } => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    Block::Paragraph { inlines } => {
                        assert!(
                            inlines
                                .iter()
                                .any(|i| matches!(i, Inline::Text(t) if t == "Hello world"))
                        );
                    }
                    other => panic!("expected Paragraph inside Blockquote, got {:?}", other),
                }
            }
            other => panic!("expected Blockquote, got {:?}", other),
        }
    }

    #[test]
    fn parses_nested_blockquote() {
        let doc = parse("> outer\n> > inner");
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Blockquote { blocks } => {
                // Should contain the outer paragraph and a nested blockquote.
                assert!(
                    blocks.iter().any(|b| matches!(b, Block::Blockquote { .. })),
                    "expected a nested Blockquote, got {:?}",
                    blocks,
                );
            }
            other => panic!("expected Blockquote, got {:?}", other),
        }
    }

    #[test]
    fn parses_strikethrough() {
        let doc = parse("~~deleted~~");
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Paragraph { inlines } => {
                assert!(
                    inlines
                        .iter()
                        .any(|i| matches!(i, Inline::Strikethrough(_)))
                );
            }
            other => panic!("expected Paragraph, got {:?}", other),
        }
    }

    #[test]
    fn parses_table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |";
        let doc = parse(md);
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Table {
                headers,
                alignments,
                rows,
            } => {
                assert_eq!(headers.len(), 2);
                assert_eq!(alignments.len(), 2);
                assert_eq!(rows.len(), 2);
                // First header cell should contain "A".
                assert!(
                    headers[0]
                        .iter()
                        .any(|i| matches!(i, Inline::Text(t) if t == "A"))
                );
                // First body cell should contain "1".
                assert!(
                    rows[0][0]
                        .iter()
                        .any(|i| matches!(i, Inline::Text(t) if t == "1"))
                );
            }
            other => panic!("expected Table, got {:?}", other),
        }
    }

    #[test]
    fn parses_image() {
        let doc = parse("![logo](https://example.com/logo.png)");
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Paragraph { inlines } => {
                assert!(inlines.iter().any(|i| matches!(
                    i,
                    Inline::Image { alt, url }
                        if alt == "logo" && url == "https://example.com/logo.png"
                )));
            }
            other => panic!("expected Paragraph, got {:?}", other),
        }
    }

    #[test]
    fn parses_footnote_reference_and_definition() {
        let doc = parse("Hello[^note].\n\n[^note]: Footnote **body**.");

        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Paragraph { inlines } => {
                assert!(inlines.iter().any(|inline| matches!(
                    inline,
                    Inline::FootnoteReference { label, index }
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
    fn parses_task_list_checked() {
        let doc = parse("- [x] done");
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::UnorderedList { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].checked, Some(true));
            }
            other => panic!("expected UnorderedList, got {:?}", other),
        }
    }

    #[test]
    fn parses_task_list_unchecked() {
        let doc = parse("- [ ] todo");
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::UnorderedList { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].checked, Some(false));
            }
            other => panic!("expected UnorderedList, got {:?}", other),
        }
    }

    #[test]
    fn parses_indented_code_block() {
        let doc = parse("    let x = 42;\n");
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::CodeBlock { language, code } => {
                assert_eq!(*language, None);
                assert!(code.contains("let x = 42;"));
            }
            other => panic!("expected CodeBlock, got {:?}", other),
        }
    }
}
