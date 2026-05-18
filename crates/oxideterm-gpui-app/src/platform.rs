use gpui::{KeyBinding, Menu, MenuItem, SystemMenuType};
pub use oxideterm_gpui_platform::window_options;
use oxideterm_i18n::{I18n, Locale};
use oxideterm_settings::PersistedSettings;

use crate::{
    CloseOtherTabs, ClosePane, CloseTab, CommandPalette, Copy, Find, FindNext, FindPrev,
    FontDecrease, FontIncrease, FontReset, NewConnection, NewTerminal, NextTab, OpenSettings,
    PaletteAiSidebar, PaletteBroadcast, PaletteCancelReconnect, PaletteCleanupDead,
    PaletteDetachTerminal, PaletteDisconnectAll, PaletteEventLog, PaletteHealthCheck,
    PaletteReconnectAll, PaletteResetPanes, Paste, PrevTab, Quit, ShellLauncher, ShowShortcuts,
    SplitHorizontal, SplitVertical, SwitchLocaleChinese, SwitchLocaleEnglish, SwitchLocaleFrench,
    SwitchLocaleGerman, SwitchLocaleItalian, SwitchLocaleJapanese, SwitchLocaleKorean,
    SwitchLocalePortugueseBrazil, SwitchLocaleSpanish, SwitchLocaleTraditionalChinese,
    SwitchLocaleVietnamese, TerminalRecording, ToggleSidebar, ZenMode,
};

pub(crate) fn app_menus(i18n: &I18n) -> Vec<Menu> {
    vec![
        Menu {
            name: i18n.t("menu.app").into(),
            items: vec![
                MenuItem::os_submenu(i18n.t("menu.services"), SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(i18n.t("command_palette.title"), CommandPalette),
                MenuItem::action(i18n.t("menu.settings"), OpenSettings),
                MenuItem::action(i18n.t("command_palette.cmd_show_shortcuts"), ShowShortcuts),
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
                MenuItem::action(i18n.t("command_palette.cmd_new_terminal"), NewTerminal),
                MenuItem::action(i18n.t("command_palette.cmd_shell_launcher"), ShellLauncher),
                MenuItem::action(i18n.t("command_palette.cmd_new_connection"), NewConnection),
                MenuItem::separator(),
                MenuItem::action(i18n.t("menu.split_horizontal"), SplitHorizontal),
                MenuItem::action(i18n.t("menu.split_vertical"), SplitVertical),
                MenuItem::action(i18n.t("menu.close_pane"), ClosePane),
                MenuItem::separator(),
                MenuItem::action(
                    i18n.t("command_palette.cmd_broadcast_toggle"),
                    PaletteBroadcast,
                ),
                MenuItem::action(
                    i18n.t("settings_view.keybindings.actions.terminal.recording"),
                    TerminalRecording,
                ),
                MenuItem::action(
                    i18n.t("command_palette.cmd_detach_terminal"),
                    PaletteDetachTerminal,
                ),
                MenuItem::action(
                    i18n.t("command_palette.cmd_cleanup_dead"),
                    PaletteCleanupDead,
                ),
                MenuItem::separator(),
                MenuItem::action(i18n.t("command_palette.cmd_reset_panes"), PaletteResetPanes),
            ],
        },
        Menu {
            name: i18n.t("menu.view").into(),
            items: vec![
                MenuItem::action(i18n.t("command_palette.title"), CommandPalette),
                MenuItem::action(i18n.t("command_palette.cmd_toggle_sidebar"), ToggleSidebar),
                MenuItem::action(i18n.t("command_palette.cmd_toggle_panel"), PaletteEventLog),
                MenuItem::action(
                    i18n.t("command_palette.cmd_toggle_ai_sidebar"),
                    PaletteAiSidebar,
                ),
                MenuItem::separator(),
                MenuItem::action(i18n.t("command_palette.cmd_font_increase"), FontIncrease),
                MenuItem::action(i18n.t("command_palette.cmd_font_decrease"), FontDecrease),
                MenuItem::action(i18n.t("command_palette.cmd_font_reset"), FontReset),
                MenuItem::separator(),
                MenuItem::action(i18n.t("command_palette.cmd_zen_mode"), ZenMode),
            ],
        },
        Menu {
            name: i18n.t("command_palette.cmd_sidebar_connections").into(),
            items: vec![
                MenuItem::action(
                    i18n.t("command_palette.cmd_disconnect_all"),
                    PaletteDisconnectAll,
                ),
                MenuItem::action(
                    i18n.t("command_palette.cmd_reconnect_all"),
                    PaletteReconnectAll,
                ),
                MenuItem::action(
                    i18n.t("command_palette.cmd_cancel_reconnect"),
                    PaletteCancelReconnect,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    i18n.t("command_palette.cmd_health_check"),
                    PaletteHealthCheck,
                ),
            ],
        },
        Menu {
            name: i18n.t("menu.window").into(),
            items: vec![
                MenuItem::action(i18n.t("menu.close_tab"), CloseTab),
                MenuItem::action(
                    i18n.t("command_palette.cmd_close_other_tabs"),
                    CloseOtherTabs,
                ),
                MenuItem::separator(),
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

pub(crate) fn app_key_bindings(settings: &PersistedSettings) -> Vec<KeyBinding> {
    crate::keybindings::startup_key_bindings(&settings.keybindings.overrides)
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
