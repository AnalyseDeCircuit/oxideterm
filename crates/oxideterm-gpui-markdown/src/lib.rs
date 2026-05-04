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
//! - Blockquotes
//! - GFM tables
//! - Ordered and unordered lists with task list checkboxes
//! - Footnotes
//! - Links (visual only) and local/remote images via GPUI async image cache
//! - Horizontal rules
//! - Smart punctuation

pub mod highlight;
pub mod model;
pub mod options;
pub mod parser;
pub mod render;
pub mod style;

pub use model::MarkdownDocument;
pub use options::MarkdownOptions;

use gpui::AnyElement;
use oxideterm_theme::ThemeTokens;

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
    let document = parser::parse(source);
    render::render_document(&document, tokens, opts)
}
