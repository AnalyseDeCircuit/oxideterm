use gpui::{BoxShadow, Div, Hsla, Rgba, Styled, point, px, rgb, rgba};

const TAURI_CARD_DARK_SHADOW_1_ALPHA: u32 = 0x66; // Tauri --theme-card-shadow rgba(0,0,0,0.4).
const TAURI_CARD_DARK_SHADOW_2_ALPHA: u32 = 0x40; // Tauri --theme-card-shadow rgba(0,0,0,0.25).
const TAURI_CARD_LIGHT_SHADOW_1_ALPHA: u32 = 0x14; // Tauri light --theme-card-shadow rgba(0,0,0,0.08).
const TAURI_CARD_LIGHT_SHADOW_2_ALPHA: u32 = 0x0d; // Tauri light --theme-card-shadow rgba(0,0,0,0.05).
const TAURI_CARD_LIGHT_LUMA_THRESHOLD: f32 = 0.55;

pub fn color_with_alpha(color: u32, alpha: u32) -> Rgba {
    rgba((color << 8) | alpha)
}

pub fn color_for_background(color: u32, has_background: bool, background_alpha: u32) -> Rgba {
    if has_background {
        color_with_alpha(color, background_alpha)
    } else {
        rgb(color)
    }
}

pub fn color_for_background_or_alpha(
    color: u32,
    has_background: bool,
    background_alpha: u32,
    plain_alpha: u32,
) -> Rgba {
    if has_background {
        color_with_alpha(color, background_alpha)
    } else {
        color_with_alpha(color, plain_alpha)
    }
}

pub fn color_with_background_scaled_alpha(
    color: u32,
    has_background: bool,
    alpha: u32,
    background_scale_alpha: u32,
) -> Rgba {
    let alpha = if has_background {
        scale_alpha_byte(alpha, background_scale_alpha)
    } else {
        alpha
    };
    color_with_alpha(color, alpha)
}

pub fn scale_alpha_byte(alpha: u32, scale_alpha: u32) -> u32 {
    ((alpha as f32) * (scale_alpha as f32 / 255.0)).round() as u32
}

pub fn tauri_card_surface(
    surface: Div,
    color: u32,
    has_background_image: bool,
    background_alpha: u32,
) -> Div {
    // Tauri maps bg-theme-bg-card through [data-bg-active] to a translucent
    // color-mix and globally adds --theme-card-shadow to every card. GPUI does
    // not currently expose CSS backdrop-filter, so this preserves the source
    // card opacity and elevation contract while leaving real background blur to
    // a renderer primitive.
    surface
        .bg(color_for_background(
            color,
            has_background_image,
            background_alpha,
        ))
        .shadow(tauri_card_shadow(color))
}

pub fn tauri_glass_surface_shadow(surface: Div, color: u32) -> Div {
    // Some Tauri panels keep their own slash-opacity class, but bg-theme-bg-card
    // still receives the shared --theme-card-shadow. This helper lets callers
    // preserve their existing alpha math while sharing the same elevation.
    surface.shadow(tauri_card_shadow(color))
}

pub fn tauri_card_shadow(color: u32) -> Vec<BoxShadow> {
    let (near_alpha, far_alpha) = if color_luma(color) >= TAURI_CARD_LIGHT_LUMA_THRESHOLD {
        (
            TAURI_CARD_LIGHT_SHADOW_1_ALPHA,
            TAURI_CARD_LIGHT_SHADOW_2_ALPHA,
        )
    } else {
        (
            TAURI_CARD_DARK_SHADOW_1_ALPHA,
            TAURI_CARD_DARK_SHADOW_2_ALPHA,
        )
    };
    vec![
        BoxShadow {
            color: Hsla::from(rgba(near_alpha)),
            offset: point(px(0.0), px(1.0)),
            blur_radius: px(3.0),
            spread_radius: px(0.0),
        },
        BoxShadow {
            color: Hsla::from(rgba(far_alpha)),
            offset: point(px(0.0), px(4.0)),
            blur_radius: px(12.0),
            spread_radius: px(0.0),
        },
    ]
}

fn color_luma(color: u32) -> f32 {
    let red = ((color >> 16) & 0xff) as f32 / 255.0;
    let green = ((color >> 8) & 0xff) as f32 / 255.0;
    let blue = (color & 0xff) as f32 / 255.0;
    red * 0.2126 + green * 0.7152 + blue * 0.0722
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tauri_card_shadow_uses_tauri_dark_and_light_alpha_levels() {
        let dark = tauri_card_shadow(0x1e1e22);
        let light = tauri_card_shadow(0xf0ece2);

        assert_eq!(dark.len(), 2);
        assert_eq!(light.len(), 2);
        assert_eq!(dark[0].offset, point(px(0.0), px(1.0)));
        assert_eq!(dark[1].offset, point(px(0.0), px(4.0)));
        assert_ne!(dark[0].color, light[0].color);
    }

    #[test]
    fn background_scaled_card_color_matches_tauri_data_bg_active_contract() {
        assert_eq!(color_for_background(0x1e1e22, true, 0x66), rgba(0x1e1e2266));
        assert_eq!(color_for_background(0x1e1e22, false, 0x66), rgb(0x1e1e22));
    }
}
