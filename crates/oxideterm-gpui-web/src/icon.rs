//! Official OxideTerm PNG app icons embedded for the browser host.

use gpui::{Image, ImageFormat, ImageSource};
use oxideterm_settings::AppIconVariant;
use std::sync::{Arc, OnceLock};

// macOS icon variants reserve an outer transparent Dock gutter (~8.5% per side)
// around a 425x425 squircle inside a 512x512 canvas.
pub const APP_ICON_CANVAS_TO_SQUIRCLE_SCALE: f32 = 512.0 / 425.0;
pub const APP_ICON_PREVIEW_SIZE: f32 = 42.0;

pub const APP_ICON_VARIANTS: &[AppIconVariant] = &[
    AppIconVariant::Default,
    AppIconVariant::WhiteBlue,
    AppIconVariant::WhiteGraphite,
    AppIconVariant::WhiteGreen,
    AppIconVariant::WhitePurple,
    AppIconVariant::WhiteRed,
    AppIconVariant::FilledOrange,
    AppIconVariant::FilledBlue,
    AppIconVariant::FilledGraphite,
    AppIconVariant::FilledGreen,
    AppIconVariant::FilledPurple,
    AppIconVariant::FilledRed,
];

fn variant_png_bytes(variant: AppIconVariant) -> &'static [u8] {
    match variant {
        AppIconVariant::Default => {
            include_bytes!("../../oxideterm-gpui-app/resources/icons/variants/default.png")
        }
        AppIconVariant::WhiteBlue => {
            include_bytes!("../../oxideterm-gpui-app/resources/icons/variants/white-blue.png")
        }
        AppIconVariant::WhiteGraphite => {
            include_bytes!("../../oxideterm-gpui-app/resources/icons/variants/white-graphite.png")
        }
        AppIconVariant::WhiteGreen => {
            include_bytes!("../../oxideterm-gpui-app/resources/icons/variants/white-green.png")
        }
        AppIconVariant::WhitePurple => {
            include_bytes!("../../oxideterm-gpui-app/resources/icons/variants/white-purple.png")
        }
        AppIconVariant::WhiteRed => {
            include_bytes!("../../oxideterm-gpui-app/resources/icons/variants/white-red.png")
        }
        AppIconVariant::FilledOrange => {
            include_bytes!("../../oxideterm-gpui-app/resources/icons/variants/filled-orange.png")
        }
        AppIconVariant::FilledBlue => {
            include_bytes!("../../oxideterm-gpui-app/resources/icons/variants/filled-blue.png")
        }
        AppIconVariant::FilledGraphite => {
            include_bytes!("../../oxideterm-gpui-app/resources/icons/variants/filled-graphite.png")
        }
        AppIconVariant::FilledGreen => {
            include_bytes!("../../oxideterm-gpui-app/resources/icons/variants/filled-green.png")
        }
        AppIconVariant::FilledPurple => {
            include_bytes!("../../oxideterm-gpui-app/resources/icons/variants/filled-purple.png")
        }
        AppIconVariant::FilledRed => {
            include_bytes!("../../oxideterm-gpui-app/resources/icons/variants/filled-red.png")
        }
    }
}

pub fn app_icon_source(variant: AppIconVariant) -> ImageSource {
    static CACHE: [OnceLock<Arc<Image>>; 12] = [const { OnceLock::new() }; 12];
    let slot = match variant {
        AppIconVariant::Default => 0,
        AppIconVariant::WhiteBlue => 1,
        AppIconVariant::WhiteGraphite => 2,
        AppIconVariant::WhiteGreen => 3,
        AppIconVariant::WhitePurple => 4,
        AppIconVariant::WhiteRed => 5,
        AppIconVariant::FilledOrange => 6,
        AppIconVariant::FilledBlue => 7,
        AppIconVariant::FilledGraphite => 8,
        AppIconVariant::FilledGreen => 9,
        AppIconVariant::FilledPurple => 10,
        AppIconVariant::FilledRed => 11,
    };
    let image = CACHE[slot]
        .get_or_init(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Png,
                variant_png_bytes(variant).to_vec(),
            ))
        })
        .clone();
    ImageSource::from(image)
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Update the browser tab `<link rel="icon">` to match the selected App Icon variant.
pub fn sync_browser_favicon(variant: AppIconVariant) {
    let data_uri = format!(
        "data:image/png;base64,{}",
        encode_base64(variant_png_bytes(variant))
    );
    let updater = js_sys::Function::new_with_args(
        "href",
        "let el = document.querySelector('link[rel=\"icon\"]');\
         if (!el) { el = document.createElement('link'); el.rel = 'icon'; document.head.appendChild(el); }\
         el.type = 'image/png'; el.href = href;",
    );
    let _ = updater.call1(
        &wasm_bindgen::JsValue::NULL,
        &wasm_bindgen::JsValue::from_str(&data_uri),
    );
}
