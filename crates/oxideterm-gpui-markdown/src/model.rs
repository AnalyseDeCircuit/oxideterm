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

    /// GFM table.
    Table {
        headers: Vec<Vec<Inline>>,
        alignments: Vec<TableAlignment>,
        rows: Vec<Vec<Vec<Inline>>>,
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
    pub inlines: Vec<Inline>,
    /// Nested sub-list, if any.
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
    Image { alt: String, url: String },

    /// `$...$` or `$$...$$` LaTeX math.
    Math { latex: String, display: bool },

    /// `[^label]`.
    FootnoteReference { label: String, index: usize },

    /// Soft or hard line break inside a paragraph.
    LineBreak,
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
                    Inline::Image { alt, url } => alt.capacity() + url.capacity(),
                    Inline::Math { latex, .. } => latex.capacity(),
                    Inline::FootnoteReference { label, .. } => label.capacity(),
                    Inline::LineBreak => 0,
                }) + 32
            })
            .sum::<usize>()
}

fn blocks_bytes(blocks: &Vec<Block>) -> usize {
    blocks.capacity() * std::mem::size_of::<Block>()
        + blocks
            .iter()
            .map(|block| {
                (match block {
                    Block::Heading { id, inlines, .. } => id.capacity() + inlines_bytes(inlines),
                    Block::Paragraph { inlines } => inlines_bytes(inlines),
                    Block::Html(text) => text.capacity(),
                    Block::HtmlContainer { blocks, .. } | Block::Blockquote { blocks, .. } => {
                        blocks_bytes(blocks)
                    }
                    Block::CodeBlock { language, code } => {
                        language.as_ref().map_or(0, String::capacity) + code.capacity()
                    }
                    Block::UnorderedList { items } | Block::OrderedList { items, .. } => {
                        items.capacity() * std::mem::size_of::<ListItem>()
                            + items
                                .iter()
                                .map(|item| {
                                    inlines_bytes(&item.inlines) + blocks_bytes(&item.children) + 32
                                })
                                .sum::<usize>()
                    }
                    Block::Table {
                        headers,
                        alignments,
                        rows,
                    } => {
                        headers.capacity() * std::mem::size_of::<Vec<Inline>>()
                            + headers.iter().map(inlines_bytes).sum::<usize>()
                            + alignments.capacity() * std::mem::size_of::<TableAlignment>()
                            + rows.capacity() * std::mem::size_of::<Vec<Vec<Inline>>>()
                            + rows
                                .iter()
                                .map(|row| {
                                    row.capacity() * std::mem::size_of::<Vec<Inline>>()
                                        + row.iter().map(inlines_bytes).sum::<usize>()
                                        + 32
                                })
                                .sum::<usize>()
                    }
                    Block::HorizontalRule => 0,
                }) + 32
            })
            .sum::<usize>()
}
