use gpui::{Rgba, rgb, rgba};

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
