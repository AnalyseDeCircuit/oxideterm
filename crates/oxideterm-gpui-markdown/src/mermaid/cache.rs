// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Bounded process cache shared by inline diagrams and the zoom viewer.

use super::MermaidRenderRequest;
use crate::options::MarkdownOptions;
use gpui::Image;
use oxideterm_theme::ThemeTokens;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock},
};

const MAX_ENTRIES: usize = 128;
static CACHE: OnceLock<Mutex<MermaidImageCache>> = OnceLock::new();

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
    render_mermaid_svg_scaled(source, tokens, opts, 2.0)
}

pub fn render_mermaid_svg_scaled(
    source: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    scale: f32,
) -> Result<RenderedMermaidImage, String> {
    MermaidRenderRequest::new(source, tokens, opts, scale).render()
}

pub fn render_mermaid_svg_image(
    source: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> Result<Arc<Image>, String> {
    render_mermaid_svg(source, tokens, opts).map(|rendered| rendered.image)
}

#[derive(Default)]
struct MermaidImageCache {
    images: HashMap<MermaidRenderRequest, Result<RenderedMermaidImage, String>>,
    order: VecDeque<MermaidRenderRequest>,
}

pub(super) fn get(key: &MermaidRenderRequest) -> Option<Result<RenderedMermaidImage, String>> {
    CACHE
        .get_or_init(|| Mutex::new(MermaidImageCache::default()))
        .lock()
        .ok()
        .and_then(|cache| cache.images.get(key).cloned())
}

pub(super) fn insert(key: MermaidRenderRequest, result: Result<RenderedMermaidImage, String>) {
    let Ok(mut cache) = CACHE
        .get_or_init(|| Mutex::new(MermaidImageCache::default()))
        .lock()
    else {
        return;
    };
    if !cache.images.contains_key(&key) {
        cache.order.push_back(key.clone());
    }
    cache.images.insert(key, result);
    while cache.images.len() > MAX_ENTRIES {
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
