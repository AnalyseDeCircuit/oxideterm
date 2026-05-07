use gpui::SharedString;

pub fn tauri_ui_font_family(configured_family: &str) -> SharedString {
    css_font_family_head(configured_family).unwrap_or_else(tauri_default_ui_font_family)
}

pub fn tauri_cjk_ui_font_family(configured_family: &str) -> SharedString {
    configured_family
        .split(',')
        .map(|family| family.trim().trim_matches(['"', '\'']))
        .find(|family| is_cjk_ui_font(family))
        .map(|family| SharedString::from(family.to_string()))
        .unwrap_or_else(tauri_default_cjk_ui_font_family)
}

pub fn css_font_family_head(configured_family: &str) -> Option<SharedString> {
    configured_family
        .split(',')
        .map(|family| family.trim().trim_matches(['"', '\'']))
        .find(|family| !family.is_empty())
        .map(|family| SharedString::from(family.to_string()))
}

#[cfg(target_os = "macos")]
fn tauri_default_ui_font_family() -> SharedString {
    // Tauri --font-sans falls through from unbundled Inter to -apple-system on macOS.
    SharedString::from("SF Pro Text")
}

#[cfg(target_os = "windows")]
fn tauri_default_ui_font_family() -> SharedString {
    // Tauri --font-sans falls through from unbundled Inter to the Windows UI font.
    SharedString::from("Segoe UI")
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn tauri_default_ui_font_family() -> SharedString {
    // Tauri --font-sans falls through to Roboto before the generic sans-serif family.
    SharedString::from("Roboto")
}

#[cfg(target_os = "windows")]
fn tauri_default_cjk_ui_font_family() -> SharedString {
    SharedString::from("DengXian")
}

#[cfg(target_os = "macos")]
fn tauri_default_cjk_ui_font_family() -> SharedString {
    SharedString::from("PingFang SC")
}

#[cfg(target_os = "linux")]
fn tauri_default_cjk_ui_font_family() -> SharedString {
    SharedString::from("Noto Sans CJK SC")
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn tauri_default_cjk_ui_font_family() -> SharedString {
    SharedString::from(".SystemUIFont")
}

fn is_cjk_ui_font(family: &str) -> bool {
    let lower = family.to_ascii_lowercase();
    family.contains("等线")
        || family.contains("微软雅黑")
        || family.contains("黑体")
        || family.contains("苹方")
        || family.contains("思源黑体")
        || lower.contains("dengxian")
        || lower.contains("microsoft yahei")
        || lower.contains("simhei")
        || lower.contains("pingfang")
        || lower.contains("noto sans cjk")
        || lower.contains("source han sans")
}
