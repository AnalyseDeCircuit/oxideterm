use super::{
    FORWARDS_ALPHA_TRANSPARENT, FORWARDS_BG_ACTIVE_BORDER_ALPHA,
    FORWARDS_BG_ACTIVE_BORDER_HALF_ALPHA, FORWARDS_BG_ACTIVE_HOVER_ALPHA,
    FORWARDS_BG_ACTIVE_SUNKEN_ALPHA, FORWARDS_BG_ACTIVE_THEME_ALPHA, FORWARDS_TW_ALPHA_50,
    ForwardRule, ForwardStatus, ForwardType, I18n, TW_BLACK, color_for_background,
    color_for_background_or_alpha, color_with_alpha, rgb, tauri_glass_surface_shadow,
};

#[derive(Clone, Copy)]
pub(super) enum ForwardButtonVariant {
    Primary,
    Secondary,
    Ghost,
}

#[derive(Clone, Copy)]
pub(super) enum ForwardRowCorners {
    None,
    Top,
    Bottom,
}

pub(super) fn parse_port(value: &str) -> Option<u16> {
    let trimmed = value.trim();
    if trimmed.is_empty() || !trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    trimmed.parse::<u16>().ok().filter(|port| *port > 0)
}

fn forwards_theme_bg(color: u32, has_background: bool) -> gpui::Rgba {
    color_for_background(color, has_background, FORWARDS_BG_ACTIVE_THEME_ALPHA)
}

pub(super) fn forwards_theme_panel_bg(color: u32, has_background: bool) -> gpui::Rgba {
    forwards_theme_bg(color, has_background)
}

pub(super) fn forwards_theme_card_bg(color: u32, has_background: bool) -> gpui::Rgba {
    forwards_theme_bg(color, has_background)
}

pub(super) fn forwards_theme_card_surface(surface: gpui::Div, color: u32) -> gpui::Div {
    // Forwards tables use Tauri bg-theme-bg-card, so keep the shared
    // --theme-card-shadow separate from the per-page background alpha helper.
    tauri_glass_surface_shadow(surface, color)
}

pub(super) fn forwards_theme_sunken_bg(color: u32, has_background: bool) -> gpui::Rgba {
    color_for_background(color, has_background, FORWARDS_BG_ACTIVE_SUNKEN_ALPHA)
}

pub(super) fn forwards_theme_hover_bg(color: u32, has_background: bool) -> gpui::Rgba {
    color_for_background(color, has_background, FORWARDS_BG_ACTIVE_HOVER_ALPHA)
}

pub(super) fn forwards_theme_border(color: u32, has_background: bool) -> gpui::Rgba {
    color_for_background(color, has_background, FORWARDS_BG_ACTIVE_BORDER_ALPHA)
}

pub(super) fn forwards_theme_border_half(color: u32, has_background: bool) -> gpui::Rgba {
    color_for_background_or_alpha(
        color,
        has_background,
        FORWARDS_BG_ACTIVE_BORDER_HALF_ALPHA,
        FORWARDS_TW_ALPHA_50,
    )
}

pub(super) fn forwards_theme_with_alpha(color: u32, alpha: u32) -> gpui::Rgba {
    color_with_alpha(color, alpha)
}

pub(super) fn forwards_palette_color(color: u32) -> gpui::Rgba {
    rgb(color)
}

pub(super) fn forwards_palette_alpha(color: u32, alpha: u32) -> gpui::Rgba {
    color_with_alpha(color, alpha)
}

pub(super) fn forwards_text_has_cjk(text: &str) -> bool {
    text.chars().any(|ch| {
        matches!(
            ch as u32,
            0x3400..=0x4dbf
                | 0x4e00..=0x9fff
                | 0xf900..=0xfaff
                | 0x20000..=0x2a6df
                | 0x2a700..=0x2b73f
                | 0x2b740..=0x2b81f
                | 0x2b820..=0x2ceaf
                | 0x30000..=0x3134f
        )
    })
}

pub(super) fn forwards_transparent() -> gpui::Rgba {
    forwards_palette_alpha(TW_BLACK, FORWARDS_ALPHA_TRANSPARENT)
}

pub(super) fn forward_addresses(rule: &ForwardRule) -> (String, String) {
    match rule.forward_type {
        ForwardType::Remote => (
            format!("{}:{}", rule.target_host, rule.target_port),
            format!("{}:{}", rule.bind_address, rule.bind_port),
        ),
        ForwardType::Local | ForwardType::Dynamic => (
            format!("{}:{}", rule.bind_address, rule.bind_port),
            format!("{}:{}", rule.target_host, rule.target_port),
        ),
    }
}

pub(super) fn forward_type_key(forward_type: ForwardType, i18n: &I18n) -> String {
    match forward_type {
        ForwardType::Local => i18n.t("forwards.type.local"),
        ForwardType::Remote => i18n.t("forwards.type.remote"),
        ForwardType::Dynamic => i18n.t("forwards.type.dynamic"),
    }
}

pub(super) fn forward_type_label(rule: ForwardRule, i18n: &I18n) -> String {
    forward_type_key(rule.forward_type, i18n)
}

pub(super) fn forward_status_key(status: &ForwardStatus) -> &'static str {
    match status {
        ForwardStatus::Starting => "forwards.status.starting",
        ForwardStatus::Active => "forwards.status.active",
        ForwardStatus::Stopped => "forwards.status.stopped",
        ForwardStatus::Error => "forwards.status.error",
        ForwardStatus::Suspended => "forwards.status.suspended",
    }
}

pub(super) fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut index = 0;
    while value >= 1024.0 && index + 1 < units.len() {
        value /= 1024.0;
        index += 1;
    }
    format!("{value:.1} {}", units[index])
}
