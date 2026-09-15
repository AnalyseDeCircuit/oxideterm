use std::sync::Arc;

use gpui::{Image, ImageFormat};
use mermaid_rs_renderer::{
    RenderOptions, Theme, compute_layout, measure_svg_dimensions, parse_mermaid_strict,
};
use oxideterm_theme::ThemeTokens;

use super::{RenderedMermaidImage, cache};
use crate::options::MarkdownOptions;

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_SVG_BYTES: usize = 512 * 1024;
const MAX_PIXEL_DIMENSION: f32 = 16_384.0;

/// Owned rendering input; no GPUI scroll handles or UI callbacks cross into a worker.
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct MermaidRenderRequest {
    source: String,
    source_too_large: bool,
    text: u32,
    muted: u32,
    border: u32,
    panel: u32,
    elevated: u32,
    accent: u32,
    font_size: u32,
    font_family: String,
    transparent: bool,
    raster_scale: u32,
}

impl MermaidRenderRequest {
    pub fn new(
        source: &str,
        tokens: &ThemeTokens,
        opts: &MarkdownOptions,
        raster_scale: f32,
    ) -> Self {
        Self {
            source: if source.len() <= MAX_SOURCE_BYTES {
                source.to_string()
            } else {
                String::new()
            },
            source_too_large: source.len() > MAX_SOURCE_BYTES,
            text: tokens.ui.text,
            muted: tokens.ui.text_muted,
            border: tokens.ui.border,
            panel: tokens.ui.bg_panel,
            elevated: tokens.ui.bg_elevated,
            accent: tokens.ui.accent,
            font_size: opts.base_font_size.to_bits(),
            font_family: opts.body_font_family.clone(),
            transparent: opts.background_surface_active,
            raster_scale: raster_scale.clamp(1.0, 4.0).to_bits(),
        }
    }

    pub(crate) fn cached(&self) -> Option<Result<RenderedMermaidImage, String>> {
        cache::get(self)
    }

    pub fn render(&self) -> Result<RenderedMermaidImage, String> {
        if let Some(result) = self.cached() {
            return result;
        }
        let result = self.render_uncached();
        cache::insert(self.clone(), result.clone());
        result
    }

    fn render_uncached(&self) -> Result<RenderedMermaidImage, String> {
        if self.source_too_large || self.source.lines().count() > 240 {
            return Err("Mermaid diagram source is too large".into());
        }
        let parsed = parse_mermaid_strict(&self.source).map_err(|error| error.to_string())?;
        if parsed.graph.nodes.len() > 180 || parsed.graph.edges.len() > 320 {
            return Err("Mermaid diagram has too many nodes or edges".into());
        }
        let options = RenderOptions {
            theme: self.theme(),
            ..RenderOptions::default()
        };
        let layout = compute_layout(&parsed.graph, &options.theme, &options.layout);
        let dimensions = measure_svg_dimensions(&layout, &options.layout, None);
        let scale = f32::from_bits(self.raster_scale);
        let width = dimensions.width * scale;
        let height = dimensions.height * scale;
        if !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
            || width > MAX_PIXEL_DIMENSION
            || height > MAX_PIXEL_DIMENSION
        {
            return Err("Mermaid diagram dimensions are too large".into());
        }
        // Explicit pixel dimensions retain the logical viewBox while rasterizing sharply on HiDPI.
        let svg = mermaid_rs_renderer::render::render_svg_with_dimensions(
            &layout,
            &options.theme,
            &options.layout,
            Some((width, height)),
        );
        if svg.len() > MAX_SVG_BYTES {
            return Err("Mermaid SVG output is too large".into());
        }
        Ok(RenderedMermaidImage {
            image: Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())),
            display_width: dimensions.width,
            display_height: dimensions.height,
        })
    }

    fn theme(&self) -> Theme {
        let color = |value: u32| format!("#{value:06x}");
        let surface = |value: u32| {
            if self.transparent {
                format!(
                    "rgba({},{},{},0.65)",
                    value >> 16 & 255,
                    value >> 8 & 255,
                    value & 255
                )
            } else {
                color(value)
            }
        };
        let mut theme = Theme::modern();
        theme.font_family = format!(
            "{}, system-ui, 'PingFang SC', 'Microsoft YaHei', 'Noto Sans CJK SC', sans-serif",
            self.font_family
        );
        theme.font_size = f32::from_bits(self.font_size);
        theme.background = "transparent".into();
        theme.primary_color = surface(self.panel);
        theme.secondary_color = surface(self.elevated);
        theme.tertiary_color = surface(self.panel);
        theme.primary_text_color = color(self.text);
        theme.text_color = color(self.text);
        theme.primary_border_color = color(self.accent);
        theme.line_color = color(self.muted);
        theme.edge_label_background = surface(self.panel);
        theme.cluster_background = surface(self.elevated);
        theme.cluster_border = color(self.border);
        theme.sequence_actor_fill = surface(self.panel);
        theme.sequence_actor_border = color(self.accent);
        theme.sequence_actor_line = color(self.muted);
        theme.sequence_note_fill = surface(self.elevated);
        theme.sequence_note_border = color(self.border);
        theme.sequence_activation_fill = surface(self.elevated);
        theme.sequence_activation_border = color(self.accent);
        theme.git_branch_label_colors = std::array::from_fn(|_| color(self.text));
        theme.git_commit_label_color = color(self.text);
        theme.git_commit_label_background = surface(self.panel);
        theme.git_tag_label_color = color(self.text);
        theme.git_tag_label_background = surface(self.elevated);
        theme.git_tag_label_border = color(self.border);
        theme.pie_title_text_size = theme.font_size * 1.2;
        theme.pie_section_text_size = theme.font_size;
        theme.pie_legend_text_size = theme.font_size;
        theme.pie_title_text_color = color(self.text);
        theme.pie_section_text_color = color(self.text);
        theme.pie_legend_text_color = color(self.text);
        theme.pie_stroke_color = color(self.border);
        theme.pie_outer_stroke_color = color(self.border);
        theme
    }
}
