// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Native LaTeX math rendering through RaTeX.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock},
};

use gpui::{Image, ImageFormat};
use oxideterm_theme::ThemeTokens;
use ratex_layout::{LayoutOptions, layout, to_display_list};
use ratex_svg::{SvgOptions, render_to_svg};
use ratex_types::{color::Color, math_style::MathStyle};

use crate::options::MarkdownOptions;

const MATH_IMAGE_CACHE_MAX_ENTRIES: usize = 256;
const MATH_SVG_RASTER_SCALE: f64 = 2.0;
static MATH_IMAGE_CACHE: OnceLock<Mutex<MathImageCache>> = OnceLock::new();

#[derive(Clone)]
pub struct RenderedMathImage {
    pub image: Arc<Image>,
    pub display_width: f32,
    pub display_height: f32,
}

pub fn render_math_svg(
    latex: &str,
    display: bool,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> Result<RenderedMathImage, String> {
    let key = MathCacheKey::new(latex, display, tokens, opts);
    if let Some(image) = math_cache_get(&key) {
        return Ok(image);
    }

    let image = render_math_svg_uncached(latex, display, tokens, opts)?;
    math_cache_insert(key, image.clone());
    Ok(image)
}

pub fn render_math_svg_image(
    latex: &str,
    display: bool,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> Result<Arc<Image>, String> {
    render_math_svg(latex, display, tokens, opts).map(|rendered| rendered.image)
}

fn render_math_svg_uncached(
    latex: &str,
    display: bool,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> Result<RenderedMathImage, String> {
    let ast = ratex_parser::parse(latex).map_err(|error| format!("RaTeX parse error: {error}"))?;
    let style = if display {
        MathStyle::Display
    } else {
        MathStyle::Text
    };
    let layout_opts = LayoutOptions::default()
        .with_style(style)
        .with_color(ratex_color(tokens.ui.text));
    let lbox = layout(&ast, &layout_opts);
    let display_list = to_display_list(&lbox);
    let logical_font_size = if display {
        opts.base_font_size * opts.math_display_scale
    } else {
        opts.base_font_size * opts.math_inline_scale
    } as f64;
    let logical_padding = if display {
        opts.math_display_padding as f64
    } else {
        opts.math_inline_padding as f64
    };
    let display_width = (display_list.width * logical_font_size + 2.0 * logical_padding) as f32;
    let display_height = ((display_list.height + display_list.depth) * logical_font_size
        + 2.0 * logical_padding) as f32;
    let svg = render_to_svg(
        &display_list,
        &SvgOptions {
            font_size: logical_font_size * MATH_SVG_RASTER_SCALE,
            padding: logical_padding * MATH_SVG_RASTER_SCALE,
            stroke_width: opts.math_stroke_width as f64 * MATH_SVG_RASTER_SCALE,
            embed_glyphs: true,
            font_dir: String::new(),
        },
    );
    Ok(RenderedMathImage {
        image: Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())),
        display_width,
        display_height,
    })
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct MathCacheKey {
    latex: String,
    display: bool,
    text_color: u32,
    base_font_size: u32,
    inline_scale: u32,
    display_scale: u32,
    inline_padding: u32,
    display_padding: u32,
    stroke_width: u32,
}

impl MathCacheKey {
    fn new(latex: &str, display: bool, tokens: &ThemeTokens, opts: &MarkdownOptions) -> Self {
        Self {
            latex: latex.to_string(),
            display,
            text_color: tokens.ui.text,
            base_font_size: opts.base_font_size.to_bits(),
            inline_scale: opts.math_inline_scale.to_bits(),
            display_scale: opts.math_display_scale.to_bits(),
            inline_padding: opts.math_inline_padding.to_bits(),
            display_padding: opts.math_display_padding.to_bits(),
            stroke_width: opts.math_stroke_width.to_bits(),
        }
    }
}

#[derive(Default)]
struct MathImageCache {
    images: HashMap<MathCacheKey, RenderedMathImage>,
    insertion_order: VecDeque<MathCacheKey>,
}

fn math_cache() -> &'static Mutex<MathImageCache> {
    MATH_IMAGE_CACHE.get_or_init(|| Mutex::new(MathImageCache::default()))
}

fn math_cache_get(key: &MathCacheKey) -> Option<RenderedMathImage> {
    math_cache().lock().ok()?.images.get(key).cloned()
}

fn math_cache_insert(key: MathCacheKey, image: RenderedMathImage) {
    let Ok(mut cache) = math_cache().lock() else {
        return;
    };
    if !cache.images.contains_key(&key) {
        cache.insertion_order.push_back(key.clone());
    }
    cache.images.insert(key, image);

    while cache.images.len() > MATH_IMAGE_CACHE_MAX_ENTRIES {
        let Some(oldest) = cache.insertion_order.pop_front() else {
            break;
        };
        cache.images.remove(&oldest);
    }
}

fn ratex_color(hex: u32) -> Color {
    let r = ((hex >> 16) & 0xff) as f32 / 255.0;
    let g = ((hex >> 8) & 0xff) as f32 / 255.0;
    let b = (hex & 0xff) as f32 / 255.0;
    Color::rgb(r, g, b)
}

#[cfg(test)]
mod tests {
    use oxideterm_theme::default_tokens;

    use super::*;

    #[test]
    fn renders_inline_math_to_svg_image() {
        let tokens = default_tokens();
        let rendered = render_math_svg(
            r"\frac{1}{2} + \sqrt{x}",
            false,
            &tokens,
            &MarkdownOptions::from_theme(&tokens),
        )
        .expect("formula should render");

        assert_eq!(rendered.image.format, ImageFormat::Svg);
        assert!(rendered.display_width > 0.0);
        assert!(rendered.display_height > 0.0);
        let svg = String::from_utf8(rendered.image.bytes.clone()).expect("svg should be utf-8");
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("<path") || svg.contains("<text"));
    }

    #[test]
    fn renders_math_svg_at_higher_internal_resolution() {
        let tokens = default_tokens();
        let rendered = render_math_svg(
            r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
            true,
            &tokens,
            &MarkdownOptions::from_theme(&tokens),
        )
        .expect("formula should render");

        let svg = String::from_utf8(rendered.image.bytes.clone()).expect("svg should be utf-8");
        let width = svg_attr(&svg, "width").expect("width attr should exist");
        let height = svg_attr(&svg, "height").expect("height attr should exist");

        assert!(width >= rendered.display_width as f64 * 1.9);
        assert!(height >= rendered.display_height as f64 * 1.9);
    }

    #[test]
    fn reuses_cached_math_image() {
        let tokens = default_tokens();
        let opts = MarkdownOptions::from_theme(&tokens);
        let first = render_math_svg_image(r"E = mc^2", true, &tokens, &opts)
            .expect("formula should render");
        let second = render_math_svg_image(r"E = mc^2", true, &tokens, &opts)
            .expect("formula should be cached");

        assert!(Arc::ptr_eq(&first, &second));
    }

    fn svg_attr(svg: &str, name: &str) -> Option<f64> {
        let needle = format!(r#"{name}=""#);
        let start = svg.find(&needle)? + needle.len();
        let end = svg[start..].find('"')? + start;
        svg[start..end].parse().ok()
    }
}
