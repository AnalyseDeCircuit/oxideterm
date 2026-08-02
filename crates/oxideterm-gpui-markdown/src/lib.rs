// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! # oxideterm-gpui-markdown
//!
//! A basic GPUI markdown rendering component for OxideTerm.
//!
//! ## Usage
//!
//! ```ignore
//! use oxideterm_gpui_markdown::{markdown, MarkdownOptions};
//! use oxideterm_theme::default_tokens;
//!
//! let tokens = default_tokens();
//! let element = markdown(&tokens, "# Hello **world**");
//! ```
//!
//! ## Supported Features
//!
//! - Headings (h1 – h6)
//! - Paragraphs
//! - Bold / italic / inline code / strikethrough
//! - Fenced code blocks with syntax highlighting (syntect)
//! - Mermaid subset diagrams (`graph` / `flowchart` TD/BT/LR/RL, `sequenceDiagram`, `pie`, and `gantt`)
//! - Blockquotes
//! - GFM tables
//! - GFM callouts (`[!NOTE]`, `[!WARNING]`, etc.)
//! - Ordered and unordered lists with task list checkboxes
//! - Footnotes
//! - Hidden YAML/TOML-style frontmatter metadata
//! - Generated heading IDs and safe fragment-link handling
//! - Clickable links and local/remote images via GPUI async image cache
//! - Link/image scheme allowlists for untrusted markdown surfaces
//! - Safe native inline HTML (`a`, `img`, `span`, `br`, emphasis, code, keyboard,
//!   underline, highlight, subscript, and superscript)
//! - Safe native block HTML (headings, containers, lists, quotes, preformatted
//!   code, tables, details content, and alignment); scripts are never executed
//!   and CSS is ignored
//! - Bare `http://` / `https://` URL autolinks
//! - Horizontal rules
//! - Smart punctuation

pub mod highlight;
mod html;
pub mod layout;
pub mod math;
pub mod mermaid;
pub mod model;
pub mod options;
pub mod parser;
pub mod render;
pub mod source;
pub mod style;

pub use layout::{MarkdownBlockLayout, MarkdownLayoutItem};
pub use model::MarkdownDocument;
pub use options::MarkdownOptions;
pub use parser::MarkdownDocumentContext;
pub use render::{MarkdownCodeBlockActions, MarkdownMermaidZoomHandler};
pub use source::{
    MarkdownSourceBlock, MarkdownSourceBlockKind, MarkdownSourceDocument, parse_source_blocks,
};

use gpui::{AnyElement, ElementId, ScrollHandle};
use oxideterm_theme::ThemeTokens;

pub type MarkdownVirtualListScrollHandle = ScrollHandle;

/// Parse and render markdown source into a GPUI element tree.
///
/// This is the primary entry point.  It parses the source into an
/// OxideTerm-owned model and immediately renders it using the given
/// theme tokens and default options.
pub fn markdown(tokens: &ThemeTokens, source: &str) -> AnyElement {
    markdown_with_options(tokens, source, &MarkdownOptions::from_theme(tokens))
}

/// Parse and render markdown source with custom options.
pub fn markdown_with_options(
    tokens: &ThemeTokens,
    source: &str,
    opts: &MarkdownOptions,
) -> AnyElement {
    let document = parser::parse_with_smart_punctuation(source, opts.enable_smart_punctuation);
    render::render_document(&document, tokens, opts)
}

/// Renders one source block with document-level reference definitions.
pub fn markdown_block_with_document_context(
    tokens: &ThemeTokens,
    source: &str,
    document_context: &MarkdownDocumentContext,
    opts: &MarkdownOptions,
) -> AnyElement {
    let document = parser::parse_with_document_context(
        source,
        document_context,
        opts.enable_smart_punctuation,
    );
    render::render_document(&document, tokens, opts)
}

/// Parse and render markdown with block-level virtual scrolling.
pub fn markdown_virtual_with_options(
    id: impl Into<ElementId>,
    tokens: &ThemeTokens,
    source: &str,
    opts: &MarkdownOptions,
    scroll_handle: &MarkdownVirtualListScrollHandle,
) -> AnyElement {
    let document = parser::parse_with_smart_punctuation(source, opts.enable_smart_punctuation);
    render::render_document_virtual(id, &document, tokens, opts, scroll_handle)
}

/// Parse and render virtualized markdown with caller-provided code-block actions.
pub fn markdown_virtual_with_code_actions(
    id: impl Into<ElementId>,
    tokens: &ThemeTokens,
    source: &str,
    opts: &MarkdownOptions,
    scroll_handle: &MarkdownVirtualListScrollHandle,
    code_actions: &render::MarkdownCodeBlockActions,
) -> AnyElement {
    let document = parser::parse_with_smart_punctuation(source, opts.enable_smart_punctuation);
    render::render_document_virtual_with_code_actions(
        id,
        &document,
        tokens,
        opts,
        scroll_handle,
        Some(code_actions),
    )
}
