// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Native LaTeX math rendering through RaTeX.

use std::sync::Arc;

use gpui::{Image, ImageFormat};
use oxideterm_theme::ThemeTokens;
use ratex_layout::{LayoutOptions, layout, to_display_list};
use ratex_svg::{SvgOptions, render_to_svg};
use ratex_types::{color::Color, math_style::MathStyle};

use crate::options::MarkdownOptions;

pub fn render_math_svg_image(
    latex: &str,
    display: bool,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> Result<Arc<Image>, String> {
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
    let svg = render_to_svg(
        &display_list,
        &SvgOptions {
            font_size: if display {
                opts.base_font_size * opts.math_display_scale
            } else {
                opts.base_font_size * opts.math_inline_scale
            } as f64,
            padding: if display {
                opts.math_display_padding as f64
            } else {
                opts.math_inline_padding as f64
            },
            stroke_width: opts.math_stroke_width as f64,
            embed_glyphs: true,
            font_dir: String::new(),
        },
    );
    Ok(Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        svg.into_bytes(),
    )))
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
        let image = render_math_svg_image(
            r"\frac{1}{2} + \sqrt{x}",
            false,
            &tokens,
            &MarkdownOptions::from_theme(&tokens),
        )
        .expect("formula should render");

        assert_eq!(image.format, ImageFormat::Svg);
        let svg = String::from_utf8(image.bytes.clone()).expect("svg should be utf-8");
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("<path") || svg.contains("<text"));
    }
}
