use gpui::{
    Bounds, KeyBinding, Menu, MenuItem, Pixels, SystemMenuType, TitlebarOptions, WindowBounds,
    WindowDecorations, WindowKind, WindowOptions, point, px, size,
};
use oxideterm_i18n::{I18n, Locale};
use oxideterm_theme::UiMetrics;

use crate::{
    ClosePane, CloseSearch, CloseTab, Copy, Find, FindNext, FindPrev, GoToTab1, GoToTab2, GoToTab3,
    GoToTab4, GoToTab5, GoToTab6, GoToTab7, GoToTab8, GoToTab9, NewTerminal, NextTab, OpenSettings,
    Paste, PrevTab, Quit, SplitHorizontal, SplitVertical, SwitchLocaleChinese, SwitchLocaleEnglish,
    SwitchLocaleFrench, SwitchLocaleGerman, SwitchLocaleItalian, SwitchLocaleJapanese,
    SwitchLocaleKorean, SwitchLocalePortugueseBrazil, SwitchLocaleSpanish,
    SwitchLocaleTraditionalChinese, SwitchLocaleVietnamese,
};

pub(crate) mod rendering;
pub(crate) mod vibrancy;

pub(crate) fn window_options(bounds: Bounds<Pixels>) -> WindowOptions {
    let metrics = UiMetrics::tauri_default();
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(point(
                px(metrics.traffic_light_x),
                px(metrics.traffic_light_y),
            )),
        }),
        kind: WindowKind::Normal,
        is_movable: true,
        is_resizable: true,
        is_minimizable: true,
        window_decorations: Some(WindowDecorations::Client),
        window_min_size: Some(size(
            px(metrics.window_min_width),
            px(metrics.window_min_height),
        )),
        ..Default::default()
    }
}

pub(crate) fn app_menus(i18n: &I18n) -> Vec<Menu> {
    vec![
        Menu {
            name: i18n.t("menu.app").into(),
            items: vec![
                MenuItem::os_submenu(i18n.t("menu.services"), SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(i18n.t("menu.new_terminal"), NewTerminal),
                MenuItem::action(i18n.t("menu.settings"), OpenSettings),
                MenuItem::action(i18n.t("menu.close_tab"), CloseTab),
                MenuItem::separator(),
                MenuItem::action(i18n.t("menu.quit"), Quit),
            ],
        },
        Menu {
            name: i18n.t("menu.edit").into(),
            items: vec![
                MenuItem::action(i18n.t("menu.copy"), Copy),
                MenuItem::action(i18n.t("menu.paste"), Paste),
                MenuItem::separator(),
                MenuItem::action(i18n.t("menu.find"), Find),
                MenuItem::action(i18n.t("menu.find_next"), FindNext),
                MenuItem::action(i18n.t("menu.find_previous"), FindPrev),
            ],
        },
        Menu {
            name: i18n.t("menu.terminal").into(),
            items: vec![
                MenuItem::action(i18n.t("menu.split_horizontal"), SplitHorizontal),
                MenuItem::action(i18n.t("menu.split_vertical"), SplitVertical),
                MenuItem::action(i18n.t("menu.close_pane"), ClosePane),
            ],
        },
        Menu {
            name: i18n.t("menu.window").into(),
            items: vec![
                MenuItem::action(i18n.t("menu.next_tab"), NextTab),
                MenuItem::action(i18n.t("menu.previous_tab"), PrevTab),
            ],
        },
        Menu {
            name: i18n.t("menu.language").into(),
            items: vec![
                MenuItem::action(locale_label(i18n, Locale::En), SwitchLocaleEnglish),
                MenuItem::action(locale_label(i18n, Locale::ZhCn), SwitchLocaleChinese),
                MenuItem::action(
                    locale_label(i18n, Locale::ZhTw),
                    SwitchLocaleTraditionalChinese,
                ),
                MenuItem::action(locale_label(i18n, Locale::De), SwitchLocaleGerman),
                MenuItem::action(locale_label(i18n, Locale::EsEs), SwitchLocaleSpanish),
                MenuItem::action(locale_label(i18n, Locale::FrFr), SwitchLocaleFrench),
                MenuItem::action(locale_label(i18n, Locale::It), SwitchLocaleItalian),
                MenuItem::action(locale_label(i18n, Locale::Ja), SwitchLocaleJapanese),
                MenuItem::action(locale_label(i18n, Locale::Ko), SwitchLocaleKorean),
                MenuItem::action(
                    locale_label(i18n, Locale::PtBr),
                    SwitchLocalePortugueseBrazil,
                ),
                MenuItem::action(locale_label(i18n, Locale::Vi), SwitchLocaleVietnamese),
            ],
        },
    ]
}

pub(crate) fn app_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-t", NewTerminal, Some("Workspace")),
        KeyBinding::new("cmd-w", CloseTab, Some("Workspace")),
        KeyBinding::new("cmd-shift-d", SplitVertical, Some("Workspace")),
        KeyBinding::new("cmd-shift-e", SplitHorizontal, Some("Workspace")),
        KeyBinding::new("cmd-shift-w", ClosePane, Some("Workspace")),
        KeyBinding::new("cmd-c", Copy, Some("Workspace")),
        KeyBinding::new("cmd-v", Paste, Some("Workspace")),
        KeyBinding::new("cmd-f", Find, Some("Workspace")),
        KeyBinding::new("cmd-,", OpenSettings, Some("Workspace")),
        KeyBinding::new("cmd-g", FindNext, Some("Workspace")),
        KeyBinding::new("cmd-shift-g", FindPrev, Some("Workspace")),
        KeyBinding::new("escape", CloseSearch, Some("Workspace")),
        KeyBinding::new("cmd-}", NextTab, Some("Workspace")),
        KeyBinding::new("cmd-{", PrevTab, Some("Workspace")),
        KeyBinding::new("cmd-1", GoToTab1, Some("Workspace")),
        KeyBinding::new("cmd-2", GoToTab2, Some("Workspace")),
        KeyBinding::new("cmd-3", GoToTab3, Some("Workspace")),
        KeyBinding::new("cmd-4", GoToTab4, Some("Workspace")),
        KeyBinding::new("cmd-5", GoToTab5, Some("Workspace")),
        KeyBinding::new("cmd-6", GoToTab6, Some("Workspace")),
        KeyBinding::new("cmd-7", GoToTab7, Some("Workspace")),
        KeyBinding::new("cmd-8", GoToTab8, Some("Workspace")),
        KeyBinding::new("cmd-9", GoToTab9, Some("Workspace")),
    ]
}

fn locale_label(i18n: &I18n, locale: Locale) -> String {
    let label = match locale {
        Locale::En => i18n.t("language.english"),
        Locale::ZhCn => i18n.t("language.simplified_chinese"),
        Locale::ZhTw => i18n.t("language.traditional_chinese"),
        Locale::De => i18n.t("language.german"),
        Locale::EsEs => i18n.t("language.spanish"),
        Locale::FrFr => i18n.t("language.french"),
        Locale::It => i18n.t("language.italian"),
        Locale::Ja => i18n.t("language.japanese"),
        Locale::Ko => i18n.t("language.korean"),
        Locale::PtBr => i18n.t("language.portuguese_brazil"),
        Locale::Vi => i18n.t("language.vietnamese"),
    };
    if i18n.locale() == locale {
        format!("✓ {label}")
    } else {
        label
    }
}
