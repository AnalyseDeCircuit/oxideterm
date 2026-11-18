use std::rc::Rc;

use gpui::{App, Context, Render, ScrollHandle, Window, div, prelude::*, px, rgb};
use oxideterm_gpui_ui::{
    modal::overlay_content_boundary,
    select::{
        select_label, select_option_action, select_option_highlighted,
        select_panel_overlay_popup_with_max_height,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;
use oxideterm_settings_model::{
    SettingsSelect, ThemeTarget, custom_theme_display_name, parse_color_hex, theme_ui_colors,
};
use oxideterm_theme::{AppUiColors, BUILT_IN_THEMES, TerminalTheme, ThemeTokens, theme_by_id};

use crate::{
    OXIDE_THEME_IDS, application_theme_description, built_in_theme_exists, is_oxide_theme,
    settings_application_palette_swatch, settings_theme_palette_swatch,
};

enum ThemeSelectEntry {
    Group(&'static str),
    Theme { id: String, swatches: ThemeSwatches },
}

impl ThemeSelectEntry {
    fn theme(id: String, settings: &PersistedSettings) -> Self {
        let swatches = ThemeSwatches {
            label: custom_theme_display_name(settings, &id),
            terminal: appearance_theme_palette(settings, &id),
            ui: theme_ui_colors(settings, &id),
        };
        Self::Theme { id, swatches }
    }
}

fn appearance_theme_entries(settings: &PersistedSettings) -> Vec<ThemeSelectEntry> {
    let theme_entry = |id| ThemeSelectEntry::theme(id, settings);
    let mut entries = Vec::new();
    if !settings.custom_themes.is_empty() {
        entries.push(ThemeSelectEntry::Group(
            "settings_view.appearance.theme_group_custom",
        ));
        let mut ids: Vec<_> = settings.custom_themes.keys().cloned().collect();
        ids.sort();
        entries.extend(ids.into_iter().map(theme_entry));
    }
    entries.push(ThemeSelectEntry::Group(
        "settings_view.appearance.theme_group_oxide",
    ));
    entries.extend(
        OXIDE_THEME_IDS
            .iter()
            .filter(|id| built_in_theme_exists(id))
            .map(|id| theme_entry((*id).to_string())),
    );
    entries.push(ThemeSelectEntry::Group(
        "settings_view.appearance.theme_group_classic",
    ));
    let mut classic: Vec<_> = BUILT_IN_THEMES
        .iter()
        .filter(|theme| !is_oxide_theme(theme.id))
        .collect();
    classic.sort_by_key(|theme| theme.id);
    entries.extend(
        classic
            .into_iter()
            .map(|theme| theme_entry(theme.id.to_string())),
    );
    entries
}

fn next_theme_option<'a>(
    entries: &'a [ThemeSelectEntry],
    current: Option<&str>,
    key: &str,
) -> Option<(usize, &'a str)> {
    let options: Vec<_> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            if let ThemeSelectEntry::Theme { id, .. } = entry {
                Some((index, id.as_str()))
            } else {
                None
            }
        })
        .collect();
    let current = options
        .iter()
        .position(|(_, id)| Some(*id) == current)
        .unwrap_or(0);
    let next = match key {
        "up" | "arrowup" => current.saturating_sub(1),
        "home" => 0,
        "end" => options.len().saturating_sub(1),
        "down" | "arrowdown" => (current + 1).min(options.len().saturating_sub(1)),
        _ => return None,
    };
    options.get(next).copied()
}

pub fn appearance_theme_palette(settings: &PersistedSettings, id: &str) -> TerminalTheme {
    let mut palette = theme_by_id(id).terminal;
    let Some(colors) = settings
        .custom_themes
        .get(id)
        .and_then(|theme| theme.get("terminalColors"))
    else {
        return palette;
    };
    // Preview only needs RGB values. The full runtime parser allocates static
    // selection-color strings, which must not be repeated on hover repaint.
    for (key, target) in [
        ("background", &mut palette.background),
        ("foreground", &mut palette.foreground),
        ("cursor", &mut palette.cursor),
        ("red", &mut palette.red),
        ("green", &mut palette.green),
        ("yellow", &mut palette.yellow),
        ("blue", &mut palette.blue),
        ("magenta", &mut palette.magenta),
        ("cyan", &mut palette.cyan),
    ] {
        if let Some(color) = colors
            .get(key)
            .and_then(serde_json::Value::as_str)
            .and_then(parse_color_hex)
        {
            *target = color;
        }
    }
    palette
}

pub struct SettingsThemePickerFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub width: f32,
    pub select: SettingsSelect,
}

struct ThemeSwatches {
    label: String,
    terminal: TerminalTheme,
    ui: AppUiColors,
}

/// The picker keeps navigation and scrolling local until the user applies a theme.
/// The window still owns the anchor, backdrop and final settings write.
pub struct SettingsThemePicker {
    entries: Vec<ThemeSelectEntry>,
    selected: Option<String>,
    preview: Option<String>,
    scroll: ScrollHandle,
    settings: gpui::WeakEntity<crate::entity::SettingsWorkspaceEntity>,
    frame: Rc<dyn Fn(&Window, &App) -> Option<SettingsThemePickerFrame>>,
    apply: Rc<dyn Fn(SettingsSelect, String, &mut App)>,
}

impl SettingsThemePicker {
    pub fn new(
        settings: gpui::WeakEntity<crate::entity::SettingsWorkspaceEntity>,
        frame: Rc<dyn Fn(&Window, &App) -> Option<SettingsThemePickerFrame>>,
        apply: Rc<dyn Fn(SettingsSelect, String, &mut App)>,
    ) -> Self {
        Self {
            entries: Vec::new(),
            selected: None,
            preview: None,
            scroll: ScrollHandle::new(),
            settings,
            frame,
            apply,
        }
    }

    pub fn open(
        &mut self,
        select: SettingsSelect,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) {
        self.entries = appearance_theme_entries(settings);
        self.selected = select.selected_theme_id(settings).map(str::to_owned);
        self.preview.clone_from(&self.selected);
        self.scroll = ScrollHandle::new();
        if let Some(row) = self.entries.iter().position(|entry| {
            matches!(entry, ThemeSelectEntry::Theme { id, .. } if self.selected.as_deref() == Some(id.as_str()))
        }) {
            self.scroll.scroll_to_item(row);
        }
        cx.notify();
    }

    pub fn preview(&self) -> Option<&str> {
        self.preview.as_deref()
    }

    fn notify_preview(&self, cx: &mut Context<Self>) {
        let _ = self.settings.update(cx, |_, cx| cx.notify());
        cx.notify();
    }

    pub fn navigate(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some((row, id)) = next_theme_option(&self.entries, self.preview(), key)
            && Some(id) != self.preview()
        {
            self.preview = Some(id.to_owned());
            self.scroll.scroll_to_item(row);
            self.notify_preview(cx);
        }
    }
}

impl Render for SettingsThemePicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(SettingsThemePickerFrame {
            tokens,
            i18n,
            width,
            select,
        }) = (self.frame)(window, cx)
        else {
            return div().into_any_element();
        };
        let Some(target) = select.theme_target() else {
            return div().into_any_element();
        };
        let mut popup = select_panel_overlay_popup_with_max_height(
            &tokens,
            width,
            tokens.metrics.settings_theme_select_popup_max_height,
        )
        .w(px(width))
        .track_scroll(&self.scroll);
        for entry in &self.entries {
            match entry {
                ThemeSelectEntry::Group(key) => {
                    popup = popup.child(select_label(&tokens, i18n.t(key)))
                }
                ThemeSelectEntry::Theme {
                    id,
                    swatches: swatch,
                } => {
                    let palette = match target {
                        ThemeTarget::Application => div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(tokens.spacing.one))
                            .child(
                                div()
                                    .whitespace_normal()
                                    .text_size(px(tokens.metrics.ui_text_xs))
                                    .text_color(rgb(tokens.ui.text_muted))
                                    .child(application_theme_description(id, swatch.ui, &i18n)),
                            )
                            .child(settings_application_palette_swatch(&tokens, swatch.ui))
                            .into_any_element(),
                        ThemeTarget::Terminal => {
                            settings_theme_palette_swatch(&tokens, swatch.terminal)
                                .into_any_element()
                        }
                    };
                    let preview_id = id.clone();
                    let picker = cx.entity().downgrade();
                    let row = select_option_highlighted(
                        &tokens,
                        "",
                        self.selected.as_ref() == Some(id),
                        self.preview.as_ref() == Some(id),
                    )
                    .h_auto()
                    .min_h(px(tokens.metrics.ui_control_height))
                    .flex_none()
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .pr(px(tokens.metrics.ui_select_check_size))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(div().truncate().child(swatch.label.clone()))
                            .child(palette),
                    )
                    .on_mouse_move(move |_, _, cx| {
                        let _ = picker.update(cx, |picker, cx| {
                            if picker.preview.as_ref() != Some(&preview_id) {
                                picker.preview = Some(preview_id.clone());
                                picker.notify_preview(cx);
                            }
                        });
                    });
                    let id = id.clone();
                    let apply = self.apply.clone();
                    popup =
                        popup.child(select_option_action(row, false, false, move |_, _, cx| {
                            apply(select, id.clone(), cx);
                            cx.stop_propagation();
                        }));
                }
            }
        }
        overlay_content_boundary(popup).into_any_element()
    }
}

#[cfg(test)]
mod theme_select_tests {
    use super::*;

    #[test]
    fn theme_navigation_skips_groups_and_scrolls_to_the_actual_row() {
        let settings = PersistedSettings::default();
        let entries = [
            ThemeSelectEntry::Group("custom"),
            ThemeSelectEntry::theme("custom:one".into(), &settings),
            ThemeSelectEntry::Group("oxide"),
            ThemeSelectEntry::theme("azurite".into(), &settings),
            ThemeSelectEntry::Group("classic"),
            ThemeSelectEntry::theme("tokyo-night".into(), &settings),
        ];
        for (current, key, expected) in [
            ("custom:one", "down", (3, "azurite")),
            ("azurite", "up", (1, "custom:one")),
            ("azurite", "end", (5, "tokyo-night")),
            ("tokyo-night", "home", (1, "custom:one")),
            ("custom:one", "up", (1, "custom:one")),
            ("tokyo-night", "down", (5, "tokyo-night")),
        ] {
            assert_eq!(
                next_theme_option(&entries, Some(current), key),
                Some(expected),
                "{current}: {key}"
            );
        }
    }
}
