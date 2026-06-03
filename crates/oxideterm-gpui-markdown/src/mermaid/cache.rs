// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Small in-process cache for rendered Mermaid SVG images.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock},
};

use gpui::{Image, ImageFormat};
use oxideterm_theme::ThemeTokens;

use crate::mermaid::{layout, parser, svg};
use crate::options::MarkdownOptions;

const MERMAID_IMAGE_CACHE_MAX_ENTRIES: usize = 128;
const MERMAID_INLINE_RASTER_SCALE: f32 = 2.0;
const MERMAID_SOURCE_MAX_BYTES: usize = 64 * 1024;
const MERMAID_SVG_MAX_DIMENSION: f32 = 16_384.0;
const MERMAID_SVG_MAX_BYTES: usize = 512 * 1024;
static MERMAID_IMAGE_CACHE: OnceLock<Mutex<MermaidImageCache>> = OnceLock::new();

#[derive(Clone)]
pub struct RenderedMermaidImage {
    pub image: Arc<Image>,
    pub display_width: f32,
    pub display_height: f32,
}

pub fn render_mermaid_svg(
    source: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> Result<RenderedMermaidImage, String> {
    render_mermaid_svg_scaled(source, tokens, opts, MERMAID_INLINE_RASTER_SCALE)
}

pub fn render_mermaid_svg_scaled(
    source: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    raster_scale: f32,
) -> Result<RenderedMermaidImage, String> {
    if source.len() > MERMAID_SOURCE_MAX_BYTES {
        return Err("Mermaid diagram source is too large".to_string());
    }

    let raster_scale = raster_scale.clamp(1.0, 4.0);
    let key = MermaidCacheKey::new(source, tokens, opts, raster_scale);
    if let Some(image) = mermaid_cache_get(&key) {
        return Ok(image);
    }

    let diagram = parser::parse(source)?;
    let layout = layout::layout(diagram, opts);
    let rendered = svg::render_with_scale(&layout, tokens, opts, raster_scale);
    if rendered.pixel_width > MERMAID_SVG_MAX_DIMENSION
        || rendered.pixel_height > MERMAID_SVG_MAX_DIMENSION
    {
        return Err("Mermaid diagram dimensions are too large".to_string());
    }
    if rendered.svg.len() > MERMAID_SVG_MAX_BYTES {
        return Err("Mermaid SVG output is too large".to_string());
    }
    let image = RenderedMermaidImage {
        image: Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            rendered.svg.into_bytes(),
        )),
        display_width: rendered.width,
        display_height: rendered.height,
    };
    mermaid_cache_insert(key, image.clone());
    Ok(image)
}

pub fn render_mermaid_svg_image(
    source: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> Result<Arc<Image>, String> {
    render_mermaid_svg(source, tokens, opts).map(|rendered| rendered.image)
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct MermaidCacheKey {
    source: String,
    text_color: u32,
    muted_color: u32,
    border_color: u32,
    panel_color: u32,
    elevated_color: u32,
    accent_color: u32,
    base_font_size: u32,
    body_font_family: String,
    raster_scale: u32,
}

impl MermaidCacheKey {
    fn new(source: &str, tokens: &ThemeTokens, opts: &MarkdownOptions, raster_scale: f32) -> Self {
        Self {
            source: source.to_string(),
            text_color: tokens.ui.text,
            muted_color: tokens.ui.text_muted,
            border_color: tokens.ui.border,
            panel_color: tokens.ui.bg_panel,
            elevated_color: tokens.ui.bg_elevated,
            accent_color: tokens.ui.accent,
            base_font_size: opts.base_font_size.to_bits(),
            body_font_family: opts.body_font_family.clone(),
            raster_scale: raster_scale.to_bits(),
        }
    }
}

#[derive(Default)]
struct MermaidImageCache {
    images: HashMap<MermaidCacheKey, RenderedMermaidImage>,
    order: VecDeque<MermaidCacheKey>,
}

fn mermaid_cache_get(key: &MermaidCacheKey) -> Option<RenderedMermaidImage> {
    MERMAID_IMAGE_CACHE
        .get_or_init(|| Mutex::new(MermaidImageCache::default()))
        .lock()
        .ok()
        .and_then(|cache| cache.images.get(key).cloned())
}

fn mermaid_cache_insert(key: MermaidCacheKey, image: RenderedMermaidImage) {
    let Ok(mut cache) = MERMAID_IMAGE_CACHE
        .get_or_init(|| Mutex::new(MermaidImageCache::default()))
        .lock()
    else {
        return;
    };

    if !cache.images.contains_key(&key) {
        cache.order.push_back(key.clone());
    }
    cache.images.insert(key, image);
    while cache.images.len() > MERMAID_IMAGE_CACHE_MAX_ENTRIES {
        if let Some(oldest) = cache.order.pop_front() {
            cache.images.remove(&oldest);
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use oxideterm_theme::default_tokens;

    use crate::options::MarkdownOptions;

    use super::*;

    #[test]
    fn caches_rendered_mermaid_svg_images() {
        let tokens = default_tokens();
        let opts = MarkdownOptions::from_theme(&tokens);

        let first = render_mermaid_svg_image("graph TD\nA --> B", &tokens, &opts).unwrap();
        let second = render_mermaid_svg_image("graph TD\nA --> B", &tokens, &opts).unwrap();

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn caches_raster_scales_separately() {
        let tokens = default_tokens();
        let opts = MarkdownOptions::from_theme(&tokens);

        let normal = render_mermaid_svg_scaled("graph TD\nA --> B", &tokens, &opts, 1.0).unwrap();
        let zoomed = render_mermaid_svg_scaled("graph TD\nA --> B", &tokens, &opts, 3.0).unwrap();

        assert_eq!(normal.display_width, zoomed.display_width);
        assert_eq!(normal.display_height, zoomed.display_height);
        assert!(!Arc::ptr_eq(&normal.image, &zoomed.image));
    }
}
