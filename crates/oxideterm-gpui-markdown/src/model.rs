// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! OxideTerm-owned markdown model.
//!
//! These types are the **only** intermediate representation between
//! `pulldown-cmark` events and GPUI rendering.  Keeping them OxideTerm-owned
//! means neither the parser nor the renderer depend on each other's types.

/// A parsed markdown document — an ordered list of block-level nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownDocument {
    pub blocks: Vec<Block>,
    /// Footnote definitions ordered by their first reference in the document.
    pub footnotes: Vec<FootnoteDefinition>,
}

/// A collected footnote definition.
#[derive(Clone, Debug, PartialEq)]
pub struct FootnoteDefinition {
    pub label: String,
    pub blocks: Vec<Block>,
}

/// Block-level markdown node.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// Source coordinates belong to one parse result, never to persisted notes.
    Located { span: SourceSpan, block: Box<Block> },
    /// `# … ######`  heading with a 1-based level (1 = h1, 6 = h6).
    Heading {
        level: u8,
        id: String,
        inlines: Vec<Inline>,
    },

    /// A normal paragraph.
    Paragraph { inlines: Vec<Inline> },

    /// Unsupported raw block HTML preserved as inert text.
    Html(String),

    /// Safe HTML container with native text alignment.
    HtmlContainer {
        alignment: BlockAlignment,
        blocks: Vec<Block>,
    },

    Details {
        id: String,
        summary: Vec<Inline>,
        blocks: Vec<Block>,
        open: bool,
    },

    /// Fenced or indented code block with an optional language hint.
    CodeBlock {
        language: Option<String>,
        code: String,
    },

    /// Unordered list (`-` / `*` / `+`).
    UnorderedList { items: Vec<ListItem> },

    /// Ordered list (`1.` …).
    OrderedList { start: u64, items: Vec<ListItem> },

    /// Thematic break / horizontal rule.
    HorizontalRule,

    /// `> blockquote` — may contain nested blocks.
    Blockquote {
        kind: Option<CalloutKind>,
        blocks: Vec<Block>,
    },

    /// GFM or HTML table; block cells preserve nested paragraphs and lists.
    Table {
        headers: Vec<Vec<Block>>,
        alignments: Vec<TableAlignment>,
        rows: Vec<Vec<Vec<Block>>>,
    },
}

/// Text alignment accepted from safe block-level HTML attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockAlignment {
    Left,
    Center,
    Right,
}

/// Column alignment for GFM tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableAlignment {
    None,
    Left,
    Center,
    Right,
}

/// GitHub-flavored blockquote alert kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalloutKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

/// A single item inside an ordered or unordered list.
#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    pub source: Option<SourceSpan>,
    pub inlines: Vec<Inline>,
    /// Remaining blocks after the initial paragraph, in source order.
    pub children: Vec<Block>,
    /// Task list checkbox state: `None` = not a task item, `Some(true)` = checked,
    /// `Some(false)` = unchecked.
    pub checked: Option<bool>,
}

/// Inline-level markdown node.
#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    /// Plain text fragment.
    Text(String),

    /// `**bold**` or `__bold__`.
    Bold(Vec<Inline>),

    /// `*italic*` or `_italic_`.
    Italic(Vec<Inline>),

    /// `` `inline code` ``.
    Code(String),

    /// `[text](url)`.
    Link { text: Vec<Inline>, url: String },

    /// Raw inline HTML preserved as inert text.
    Html(String),

    /// Safe `<kbd>...</kbd>` inline HTML rendered as keyboard-style text.
    Kbd(Vec<Inline>),

    /// Safe `<sub>...</sub>` inline HTML rendered without exposing tags.
    Subscript(Vec<Inline>),

    /// Safe `<sup>...</sup>` inline HTML rendered without exposing tags.
    Superscript(Vec<Inline>),

    /// Safe `<u>...</u>` inline HTML.
    Underline(Vec<Inline>),

    /// Safe `<mark>...</mark>` inline HTML.
    Highlight(Vec<Inline>),

    /// `~~strikethrough~~`.
    Strikethrough(Vec<Inline>),

    /// `![alt](url)`.
    Image {
        alt: String,
        url: String,
        dimensions: ImageDimensions,
    },

    /// `$...$` or `$$...$$` LaTeX math.
    Math { latex: String, display: bool },

    /// `[^label]`.
    FootnoteReference {
        label: String,
        index: usize,
        occurrence: usize,
    },

    /// Soft or hard line break inside a paragraph.
    LineBreak,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImageDimensions {
    pub width: Option<ImageLength>,
    pub height: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageLength {
    Pixels(f32),
    Percent(f32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl Block {
    pub(crate) fn without_sources(&self) -> Block {
        let mut block = self.unlocated().clone();
        match &mut block {
            Block::HtmlContainer { blocks, .. }
            | Block::Blockquote { blocks, .. }
            | Block::Details { blocks, .. } => {
                *blocks = blocks.iter().map(Block::without_sources).collect();
            }
            Block::OrderedList { items, .. } | Block::UnorderedList { items } => {
                for item in items {
                    item.source = None;
                    item.children = item.children.iter().map(Block::without_sources).collect();
                }
            }
            Block::Table { headers, rows, .. } => {
                for cell in headers.iter_mut().chain(rows.iter_mut().flatten()) {
                    *cell = cell.iter().map(Block::without_sources).collect();
                }
            }
            _ => {}
        }
        block
    }
    pub fn unlocated(&self) -> &Block {
        match self {
            Self::Located { block, .. } => block.unlocated(),
            _ => self,
        }
    }
    pub fn source_span(&self) -> Option<SourceSpan> {
        match self {
            Self::Located { span, .. } => Some(*span),
            _ => None,
        }
    }
}

impl MarkdownDocument {
    /// Allocation capacities retained by the parsed tree, excluding the document value itself.
    pub fn retained_bytes(&self) -> usize {
        blocks_bytes(&self.blocks)
            + self.footnotes.capacity() * std::mem::size_of::<FootnoteDefinition>()
            + self
                .footnotes
                .iter()
                .map(|note| note.label.capacity() + blocks_bytes(&note.blocks))
                .sum::<usize>()
    }
}

fn inlines_bytes(inlines: &Vec<Inline>) -> usize {
    inlines.capacity() * std::mem::size_of::<Inline>()
        + inlines
            .iter()
            .map(|inline| {
                (match inline {
                    Inline::Text(text) | Inline::Code(text) | Inline::Html(text) => text.capacity(),
                    Inline::Bold(items)
                    | Inline::Italic(items)
                    | Inline::Kbd(items)
                    | Inline::Subscript(items)
                    | Inline::Superscript(items)
                    | Inline::Underline(items)
                    | Inline::Highlight(items)
                    | Inline::Strikethrough(items) => inlines_bytes(items),
                    Inline::Link { text, url } => inlines_bytes(text) + url.capacity(),
                    Inline::Image { alt, url, .. } => alt.capacity() + url.capacity(),
                    Inline::Math { latex, .. } => latex.capacity(),
                    Inline::FootnoteReference { label, .. } => label.capacity(),
                    Inline::LineBreak => 0,
                }) + 32
            })
            .sum::<usize>()
}

fn blocks_bytes(blocks: &Vec<Block>) -> usize {
    blocks.capacity() * std::mem::size_of::<Block>()
        + blocks.iter().map(block_payload_bytes).sum::<usize>()
}

fn block_payload_bytes(block: &Block) -> usize {
    (match block {
        Block::Located { block, .. } => std::mem::size_of::<Block>() + block_payload_bytes(block),
        Block::Heading { id, inlines, .. } => id.capacity() + inlines_bytes(inlines),
        Block::Paragraph { inlines } => inlines_bytes(inlines),
        Block::Html(text) => text.capacity(),
        Block::HtmlContainer { blocks, .. } | Block::Blockquote { blocks, .. } => {
            blocks_bytes(blocks)
        }
        Block::Details {
            id,
            summary,
            blocks,
            ..
        } => id.capacity() + inlines_bytes(summary) + blocks_bytes(blocks),
        Block::CodeBlock { language, code } => {
            language.as_ref().map_or(0, String::capacity) + code.capacity()
        }
        Block::UnorderedList { items } | Block::OrderedList { items, .. } => {
            items.capacity() * std::mem::size_of::<ListItem>()
                + items
                    .iter()
                    .map(|item| inlines_bytes(&item.inlines) + blocks_bytes(&item.children) + 32)
                    .sum::<usize>()
        }
        Block::Table {
            headers,
            alignments,
            rows,
        } => {
            headers.capacity() * std::mem::size_of::<Vec<Block>>()
                + headers.iter().map(blocks_bytes).sum::<usize>()
                + alignments.capacity() * std::mem::size_of::<TableAlignment>()
                + rows.capacity() * std::mem::size_of::<Vec<Vec<Block>>>()
                + rows
                    .iter()
                    .map(|row| {
                        row.capacity() * std::mem::size_of::<Vec<Block>>()
                            + row.iter().map(blocks_bytes).sum::<usize>()
                            + 32
                    })
                    .sum::<usize>()
        }
        Block::HorizontalRule => 0,
    }) + 32
}
