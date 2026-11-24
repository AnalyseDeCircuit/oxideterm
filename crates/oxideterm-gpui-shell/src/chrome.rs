use gpui::{div, prelude::*, px, rgb, svg};
use oxideterm_gpui_ui::{
    button::{ButtonRadius, IconButtonOptions, icon_button},
    icons::LucideIcon,
    theme_card_surface_shadow,
};
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::TabKind;

// Recess non-active context-rail buttons when AI is off or the workspace is not
// focused on a terminal/host surface, while keeping full brightness on hover.
const CONTEXT_RAIL_MUTED_IDLE_OPACITY: f32 = 0.55;

pub fn context_rail_icon_color_hex(tokens: &ThemeTokens, active: bool, dimmed: bool) -> u32 {
    if active {
        tokens.ui.accent
    } else if dimmed {
        tokens.ui.text_muted
    } else {
        tokens.ui.text
    }
}

pub fn context_rail_icon_button_options(
    tokens: &ThemeTokens,
    active_background: gpui::Rgba,
    active: bool,
    dimmed: bool,
) -> IconButtonOptions {
    let muted = !active && dimmed;
    IconButtonOptions {
        radius: ButtonRadius::Md,
        has_background: active,
        background: active.then_some(active_background),
        border: active.then(|| rgb(tokens.ui.border)),
        hover_background: Some(rgb(tokens.ui.bg_hover)),
        hover_opacity: muted.then_some(1.0),
        idle_opacity: if muted {
            CONTEXT_RAIL_MUTED_IDLE_OPACITY
        } else {
            1.0
        },
        ..IconButtonOptions::compact(tokens.metrics.activity_icon_size)
    }
}

pub fn activity_button(
    tokens: &ThemeTokens,
    icon: LucideIcon,
    active: bool,
    dimmed: bool,
    active_background: gpui::Rgba,
    badge_count: u32,
    badge_color: u32,
) -> gpui::Div {
    let badge_text_color = if badge_color == tokens.ui.accent {
        tokens.ui.accent_text
    } else {
        tokens.ui.bg
    };
    let button = icon_button(
        tokens,
        svg()
            .path(icon.path())
            .size(px(tokens.metrics.activity_icon_glyph_size))
            .text_color(rgb(context_rail_icon_color_hex(tokens, active, dimmed)))
            .into_any_element(),
        context_rail_icon_button_options(tokens, active_background, active, dimmed),
    );
    let button = if active {
        theme_card_surface_shadow(button, tokens)
    } else {
        button
    };
    button
        .relative()
        .mb(px(tokens.metrics.activity_icon_gap))
        .when(badge_count > 0, |icon_el| {
            icon_el.child(
                div()
                    .absolute()
                    // Tauri badges sit outside the `h-9 w-9` Button
                    // (`-top-1 -right-1`) so the active button chrome and
                    // numeric pill do not visually collide.
                    .right(px(-4.0))
                    .top(px(-4.0))
                    .min_w(px(14.0))
                    .h(px(14.0))
                    .px(px(3.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(rgb(badge_color))
                    .text_color(rgb(badge_text_color))
                    .text_size(px(tokens.metrics.ui_text_2xs))
                    .child(if badge_count > 99 {
                        "99+".to_string()
                    } else {
                        badge_count.to_string()
                    }),
            )
        })
}

pub fn tab_icon(kind: &TabKind) -> LucideIcon {
    match kind {
        TabKind::Workspace => LucideIcon::AppWindow,
        TabKind::LocalTerminal => LucideIcon::Square,
        TabKind::SshTerminal => LucideIcon::Terminal,
        TabKind::MoshTerminal => LucideIcon::Terminal,
        TabKind::FileManager => LucideIcon::FolderOpen,
        TabKind::Graphics | TabKind::RemoteDesktop => LucideIcon::Monitor,
        TabKind::Runtime | TabKind::ConnectionPool => LucideIcon::Gauge,
        TabKind::Topology => LucideIcon::Network,
        TabKind::NotificationCenter => LucideIcon::Bell,
        TabKind::Sftp => LucideIcon::FolderInput,
        TabKind::Ide => LucideIcon::Code2,
        TabKind::Forwards => LucideIcon::ArrowLeftRight,
        TabKind::SessionManager => LucideIcon::LayoutList,
        TabKind::PluginManager | TabKind::Plugin { .. } => LucideIcon::Puzzle,
        TabKind::CloudSync => LucideIcon::Cloud,
        TabKind::Knowledge => LucideIcon::BookOpen,
        TabKind::Settings => LucideIcon::Settings,
        TabKind::Onboarding => LucideIcon::Sparkles,
    }
}
