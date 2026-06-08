// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Configuration knobs for the markdown renderer.

use std::path::{Path, PathBuf};

use oxideterm_theme::{ThemeTokens, UiMetrics};

pub const MARKDOWN_IMAGE_CACHE_ID: &str = "oxideterm-markdown-images";

/// Options that control markdown rendering behaviour.
#[derive(Clone, Debug)]
pub struct MarkdownOptions {
    /// Base font size in pixels for body text.
    /// Heading sizes are derived as multiples of this value.
    pub base_font_size: f32,

    /// Font family used for body text.
    pub body_font_family: String,

    /// Font family used for code spans and code blocks.
    pub code_font_family: String,

    /// Per-level heading font scale, indexed as `level - 1`.
    pub heading_font_scales: [f32; 6],

    /// Inline and fenced code font-size scale relative to body text.
    pub code_font_scale: f32,

    /// Small language label font-size scale relative to code text.
    pub code_label_font_scale: f32,

    /// Footnote font-size scale relative to body text.
    pub footnote_font_scale: f32,

    /// Vertical gap between block-level elements, in pixels.
    pub block_gap: f32,

    /// Horizontal indentation per list nesting level, in pixels.
    pub list_indent: f32,

    /// Internal padding of fenced code blocks, in pixels.
    pub code_block_padding: f32,

    /// Enable GFM table rendering.
    pub enable_tables: bool,

    /// Enable task list checkbox rendering.
    pub enable_task_lists: bool,

    /// Enable smart punctuation (curly quotes, em-dashes, etc.).
    pub enable_smart_punctuation: bool,

    /// Enable footnote references and a footnote section.
    pub enable_footnotes: bool,

    /// Render images through GPUI's async image cache.
    pub enable_async_images: bool,

    /// Stable element-state ID for the markdown image cache.
    pub image_cache_id: &'static str,

    /// Maximum rendered image width in pixels.
    pub max_image_width: f32,

    /// Base directory used to resolve relative image paths.
    pub image_base_dir: Option<PathBuf>,

    /// URL schemes that the renderer may open from markdown links.
    pub allowed_link_schemes: Vec<&'static str>,

    /// URL schemes that the renderer may load from markdown images.
    pub allowed_image_schemes: Vec<&'static str>,

    /// Width of the left border on blockquotes, in pixels.
    pub blockquote_border_width: f32,

    /// Inline math font-size scale relative to body text.
    pub math_inline_scale: f32,

    /// Display math font-size scale relative to body text.
    pub math_display_scale: f32,

    /// Internal padding around inline math SVG output, in pixels.
    pub math_inline_padding: f32,

    /// Internal padding around display math SVG output, in pixels.
    pub math_display_padding: f32,

    /// Stroke width for vector math paths, in pixels.
    pub math_stroke_width: f32,

    /// Localized prefix shown before Mermaid parse/layout/render errors.
    pub mermaid_error_prefix: String,

    /// Localized action label for opening Mermaid diagrams in a larger view.
    pub mermaid_expand_label: String,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self::from_metrics(UiMetrics::tauri_default())
    }
}

impl MarkdownOptions {
    /// Build markdown renderer options from the active theme metrics.
    pub fn from_theme(tokens: &ThemeTokens) -> Self {
        Self::from_metrics(tokens.metrics)
    }

    /// Build markdown renderer options from UI metrics.
    pub fn from_metrics(metrics: UiMetrics) -> Self {
        Self {
            base_font_size: metrics.markdown_body_font_size,
            body_font_family: metrics.markdown_body_font_family.into(),
            code_font_family: metrics.markdown_code_font_family.into(),
            heading_font_scales: [
                metrics.markdown_heading_h1_scale,
                metrics.markdown_heading_h2_scale,
                metrics.markdown_heading_h3_scale,
                metrics.markdown_heading_h4_scale,
                metrics.markdown_heading_h5_scale,
                metrics.markdown_heading_h6_scale,
            ],
            code_font_scale: metrics.markdown_code_font_scale,
            code_label_font_scale: metrics.markdown_code_label_font_scale,
            footnote_font_scale: metrics.markdown_footnote_font_scale,
            block_gap: metrics.markdown_block_gap,
            list_indent: metrics.markdown_list_indent,
            code_block_padding: metrics.markdown_code_block_padding,
            enable_tables: true,
            enable_task_lists: true,
            enable_smart_punctuation: true,
            enable_footnotes: true,
            enable_async_images: true,
            image_cache_id: MARKDOWN_IMAGE_CACHE_ID,
            max_image_width: metrics.markdown_max_image_width,
            image_base_dir: None,
            allowed_link_schemes: vec!["http", "https", "mailto", "file"],
            allowed_image_schemes: vec!["http", "https", "file", "data"],
            blockquote_border_width: metrics.markdown_blockquote_border_width,
            math_inline_scale: 1.0,
            math_display_scale: 1.2,
            math_inline_padding: 1.0,
            math_display_padding: 6.0,
            math_stroke_width: 1.5,
            mermaid_error_prefix: "Unsupported Mermaid diagram".to_string(),
            mermaid_expand_label: "EXPAND".to_string(),
        }
    }

    /// Resolve relative markdown resources against the directory containing
    /// the rendered source file.
    pub fn with_source_path(mut self, source_path: impl AsRef<Path>) -> Self {
        self.image_base_dir = source_path.as_ref().parent().map(Path::to_path_buf);
        self
    }

    /// Resolve relative markdown resources against an explicit directory.
    pub fn with_image_base_dir(mut self, base_dir: impl Into<PathBuf>) -> Self {
        self.image_base_dir = Some(base_dir.into());
        self
    }
}
