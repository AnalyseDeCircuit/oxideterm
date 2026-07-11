use gpui::{BoxShadow, Div, Hsla, Rgba, Styled, div, point, prelude::*, px, rgb, rgba};
use oxideterm_theme::ThemeTokens;

const TAURI_CARD_DARK_SHADOW_1_ALPHA: u32 = 0x66; // Tauri --theme-card-shadow rgba(0,0,0,0.4).
const TAURI_CARD_DARK_SHADOW_2_ALPHA: u32 = 0x40; // Tauri --theme-card-shadow rgba(0,0,0,0.25).
const TAURI_CARD_LIGHT_SHADOW_1_ALPHA: u32 = 0x14; // Tauri light --theme-card-shadow rgba(0,0,0,0.08).
const TAURI_CARD_LIGHT_SHADOW_2_ALPHA: u32 = 0x0d; // Tauri light --theme-card-shadow rgba(0,0,0,0.05).
const TAURI_CARD_LIGHT_LUMA_THRESHOLD: f32 = 0.55;
const LOW_SURFACE_SEPARATION: f32 = 0.035;
const MEDIUM_SURFACE_SEPARATION: f32 = 0.08;
const SEMANTIC_SURFACE_STRONG_ALPHA: u32 = 0xf2;
const SEMANTIC_SURFACE_PANEL_ALPHA: u32 = 0xe6;
const SEMANTIC_SURFACE_SOFT_ALPHA: u32 = 0xcc;
const SEMANTIC_SURFACE_INSET_ALPHA: u32 = 0x99;
const SEMANTIC_SURFACE_BORDER_ALPHA: u32 = 0x80;
const SEMANTIC_SURFACE_STRONG_BORDER_ALPHA: u32 = 0x99;
const SEMANTIC_SURFACE_ACTIVE_ALPHA: u32 = 0x33;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ThemeElevationSpec {
    panel_border_alpha: u32,
    card_border_alpha: u32,
    card_near_shadow_alpha: u32,
    card_far_shadow_alpha: u32,
    overlay_near_shadow_alpha: u32,
    overlay_far_shadow_alpha: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceKind {
    Panel,
    ElevatedPopover,
    InsetGroup,
    EntityRow,
    Inspector,
    TerminalOverlay,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfacePadding {
    None,
    Compact,
    Normal,
    Spacious,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceOptions {
    pub kind: SurfaceKind,
    pub padding: SurfacePadding,
    pub active: bool,
    pub has_background_image: bool,
}

impl SurfaceOptions {
    pub const fn new(kind: SurfaceKind) -> Self {
        Self {
            kind,
            padding: default_surface_padding(kind),
            active: false,
            has_background_image: false,
        }
    }

    pub const fn padding(mut self, padding: SurfacePadding) -> Self {
        self.padding = padding;
        self
    }

    pub const fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub const fn has_background_image(mut self, has_background_image: bool) -> Self {
        self.has_background_image = has_background_image;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceChrome {
    pub background: Rgba,
    pub border: Rgba,
    pub bordered: bool,
    pub radius: f32,
    pub padding: f32,
    pub shadow_color: Option<u32>,
}

pub const fn default_surface_padding(kind: SurfaceKind) -> SurfacePadding {
    match kind {
        SurfaceKind::EntityRow => SurfacePadding::Compact,
        SurfaceKind::ElevatedPopover | SurfaceKind::TerminalOverlay => SurfacePadding::Normal,
        SurfaceKind::Panel | SurfaceKind::InsetGroup | SurfaceKind::Inspector => {
            SurfacePadding::Spacious
        }
    }
}

pub fn semantic_surface(tokens: &ThemeTokens, options: SurfaceOptions) -> Div {
    let chrome = surface_chrome(tokens, options);
    let surface = div()
        .rounded(px(chrome.radius))
        .bg(chrome.background)
        .when(chrome.bordered, |surface| {
            surface.border_1().border_color(chrome.border)
        })
        .when(chrome.padding > 0.0, |surface| {
            surface.p(px(chrome.padding))
        });

    // Semantic surfaces keep old Tauri shadows available, but the caller now
    // chooses by native surface kind instead of by migration history.
    match options.kind {
        SurfaceKind::Inspector => theme_card_surface_shadow(surface, tokens),
        SurfaceKind::ElevatedPopover | SurfaceKind::TerminalOverlay => {
            theme_overlay_surface_shadow(surface, tokens)
        }
        _ => surface,
    }
}

pub fn surface_chrome(tokens: &ThemeTokens, options: SurfaceOptions) -> SurfaceChrome {
    let theme = tokens.ui;
    let elevation = theme_elevation_spec(tokens);
    let (base, alpha, border_alpha, radius, shadow_color) = match options.kind {
        SurfaceKind::Panel => (
            theme.bg_panel,
            SEMANTIC_SURFACE_PANEL_ALPHA,
            elevation.panel_border_alpha,
            tokens.radii.lg,
            None,
        ),
        SurfaceKind::ElevatedPopover => (
            theme.bg_elevated,
            SEMANTIC_SURFACE_STRONG_ALPHA,
            elevation
                .card_border_alpha
                .max(SEMANTIC_SURFACE_STRONG_BORDER_ALPHA),
            tokens.radii.lg,
            Some(theme.bg_elevated),
        ),
        SurfaceKind::InsetGroup => (
            theme.bg_sunken,
            SEMANTIC_SURFACE_INSET_ALPHA,
            elevation.panel_border_alpha,
            tokens.radii.md,
            None,
        ),
        SurfaceKind::EntityRow => (
            if options.active {
                theme.bg_active
            } else {
                theme.bg_panel
            },
            if options.active {
                SEMANTIC_SURFACE_ACTIVE_ALPHA
            } else {
                0x00
            },
            0x00,
            tokens.radii.md,
            None,
        ),
        SurfaceKind::Inspector => (
            theme.bg_card,
            SEMANTIC_SURFACE_SOFT_ALPHA,
            elevation.card_border_alpha,
            tokens.radii.lg,
            None,
        ),
        SurfaceKind::TerminalOverlay => (
            theme.bg_elevated,
            SEMANTIC_SURFACE_STRONG_ALPHA,
            SEMANTIC_SURFACE_BORDER_ALPHA,
            tokens.radii.md,
            Some(theme.bg_elevated),
        ),
    };
    let background = if alpha == 0x00 {
        rgba(0x00000000)
    } else {
        color_for_background(base, options.has_background_image, alpha)
    };
    SurfaceChrome {
        background,
        border: if border_alpha == 0x00 {
            rgba(0x00000000)
        } else {
            color_with_alpha(theme.border, border_alpha)
        },
        bordered: border_alpha != 0x00,
        radius,
        padding: surface_padding_px(tokens, options.padding),
        shadow_color,
    }
}

pub fn surface_padding_px(tokens: &ThemeTokens, padding: SurfacePadding) -> f32 {
    match padding {
        SurfacePadding::None => 0.0,
        SurfacePadding::Compact => tokens.spacing.two,
        SurfacePadding::Normal => tokens.spacing.three,
        SurfacePadding::Spacious => tokens.spacing.three * 2.0,
    }
}

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

pub fn theme_card_surface(
    surface: Div,
    tokens: &ThemeTokens,
    has_background_image: bool,
    background_alpha: u32,
) -> Div {
    // New GPUI surfaces use the active palette for both fill and elevation;
    // the legacy color-only helper remains available for migration safety.
    let surface = surface.bg(color_for_background(
        tokens.ui.bg_card,
        has_background_image,
        background_alpha,
    ));
    theme_card_surface_shadow(surface, tokens)
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
    shadows_with_alpha(near_alpha, far_alpha, 3.0, 12.0)
}

pub fn theme_card_shadow(tokens: &ThemeTokens) -> Vec<BoxShadow> {
    let elevation = theme_elevation_spec(tokens);
    // Theme-aware cards preserve material identity while avoiding one fixed
    // dark shadow recipe across every built-in and custom palette.
    shadows_with_alpha(
        elevation.card_near_shadow_alpha,
        elevation.card_far_shadow_alpha,
        3.0,
        12.0,
    )
}

pub fn theme_overlay_shadow(tokens: &ThemeTokens) -> Vec<BoxShadow> {
    let elevation = theme_elevation_spec(tokens);
    // Floating surfaces must remain visibly above cards in every palette.
    shadows_with_alpha(
        elevation.overlay_near_shadow_alpha,
        elevation.overlay_far_shadow_alpha,
        5.0,
        18.0,
    )
}

pub fn theme_card_surface_shadow(surface: Div, tokens: &ThemeTokens) -> Div {
    surface.shadow(theme_card_shadow(tokens))
}

pub fn theme_overlay_surface_shadow(surface: Div, tokens: &ThemeTokens) -> Div {
    surface.shadow(theme_overlay_shadow(tokens))
}

fn shadows_with_alpha(
    near_alpha: u32,
    far_alpha: u32,
    near_blur: f32,
    far_blur: f32,
) -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            color: Hsla::from(rgba(near_alpha)),
            offset: point(px(0.0), px(1.0)),
            blur_radius: px(near_blur),
            spread_radius: px(0.0),
        },
        BoxShadow {
            color: Hsla::from(rgba(far_alpha)),
            offset: point(px(0.0), px(4.0)),
            blur_radius: px(far_blur),
            spread_radius: px(0.0),
        },
    ]
}

fn theme_elevation_spec(tokens: &ThemeTokens) -> ThemeElevationSpec {
    let ui = tokens.ui;
    let card_separation = (color_luma(ui.bg_card) - color_luma(ui.bg)).abs();
    let panel_separation = (color_luma(ui.bg_panel) - color_luma(ui.bg)).abs();
    let light = color_luma(ui.bg) >= TAURI_CARD_LIGHT_LUMA_THRESHOLD;

    let (card_border_alpha, panel_border_alpha) = if card_separation < LOW_SURFACE_SEPARATION {
        (0xb3, 0x8f)
    } else if card_separation < MEDIUM_SURFACE_SEPARATION {
        (0x99, 0x73)
    } else {
        (0x80, 0x66)
    };
    let panel_border_alpha = if panel_separation < LOW_SURFACE_SEPARATION {
        panel_border_alpha.max(0x80)
    } else {
        panel_border_alpha
    };
    let (card_near, card_far) = if light {
        (0x18, 0x0f)
    } else if card_separation < LOW_SURFACE_SEPARATION {
        (0x52, 0x30)
    } else {
        (0x3d, 0x24)
    };

    ThemeElevationSpec {
        panel_border_alpha,
        card_border_alpha,
        card_near_shadow_alpha: card_near,
        card_far_shadow_alpha: card_far,
        overlay_near_shadow_alpha: (card_near + 0x14).min(0x66),
        overlay_far_shadow_alpha: (card_far + 0x10).min(0x40),
    }
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

    #[test]
    fn semantic_surface_kinds_have_native_chrome_defaults() {
        let tokens = oxideterm_theme::default_tokens();

        let panel = surface_chrome(&tokens, SurfaceOptions::new(SurfaceKind::Panel));
        let popover = surface_chrome(&tokens, SurfaceOptions::new(SurfaceKind::ElevatedPopover));
        let row = surface_chrome(&tokens, SurfaceOptions::new(SurfaceKind::EntityRow));

        assert_eq!(panel.radius, tokens.radii.lg);
        assert_eq!(panel.padding, tokens.spacing.three * 2.0);
        assert!(panel.shadow_color.is_none());
        assert_eq!(popover.radius, tokens.radii.lg);
        assert_eq!(popover.padding, tokens.spacing.three);
        assert_eq!(popover.shadow_color, Some(tokens.ui.bg_elevated));
        assert_eq!(row.background, rgba(0x00000000));
        assert_eq!(row.border, rgba(0x00000000));
        assert!(!row.bordered);
        assert_eq!(row.padding, tokens.spacing.two);
    }

    #[test]
    fn active_entity_row_gets_subtle_active_background() {
        let tokens = oxideterm_theme::default_tokens();
        let row = surface_chrome(
            &tokens,
            SurfaceOptions::new(SurfaceKind::EntityRow).active(true),
        );

        assert_eq!(row.background, rgb(tokens.ui.bg_active));
        assert_eq!(row.radius, tokens.radii.md);
        assert_eq!(row.padding, tokens.spacing.two);
    }

    #[test]
    fn representative_theme_matrix_preserves_visible_card_and_overlay_hierarchy() {
        for theme_id in [
            "default",
            "oxide",
            "verdigris",
            "paper-oxide",
            "synthwave-84",
            "spring-rice",
        ] {
            let tokens = ThemeTokens::from_builtin(oxideterm_theme::theme_by_id(theme_id));
            let elevation = theme_elevation_spec(&tokens);
            let card = surface_chrome(&tokens, SurfaceOptions::new(SurfaceKind::Inspector));
            let overlay =
                surface_chrome(&tokens, SurfaceOptions::new(SurfaceKind::ElevatedPopover));

            assert!(card.bordered, "{theme_id} card border");
            assert!(overlay.bordered, "{theme_id} overlay border");
            assert!(
                elevation.overlay_near_shadow_alpha > elevation.card_near_shadow_alpha,
                "{theme_id} near shadow hierarchy"
            );
            assert!(
                elevation.overlay_far_shadow_alpha > elevation.card_far_shadow_alpha,
                "{theme_id} far shadow hierarchy"
            );
            assert!(elevation.card_near_shadow_alpha <= 0x52);
            assert!(elevation.overlay_near_shadow_alpha <= 0x66);
        }
    }

    #[test]
    fn custom_low_contrast_palette_gets_extra_surface_separation() {
        let mut tokens = oxideterm_theme::default_tokens();
        // Custom themes enter the UI as raw colors, so elevation must derive
        // from the palette rather than from a built-in theme identifier.
        tokens.ui.bg = 0x202122;
        tokens.ui.bg_panel = 0x222324;
        tokens.ui.bg_card = 0x242526;
        tokens.ui.bg_elevated = 0x292a2b;
        let elevation = theme_elevation_spec(&tokens);

        assert_eq!(elevation.card_border_alpha, 0xb3);
        assert_eq!(elevation.panel_border_alpha, 0x8f);
        assert_eq!(elevation.card_near_shadow_alpha, 0x52);
        assert!(elevation.overlay_near_shadow_alpha > elevation.card_near_shadow_alpha);
    }
}
