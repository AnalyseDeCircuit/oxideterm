use gpui::{KeyBinding, Menu, MenuItem, SystemMenuType};
pub use oxideterm_gpui_platform::{window_options, workspace_window_options_with_bounds};
use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;

use crate::{
    CloseOtherTabs, ClosePane, CloseTab, CommandPalette, Copy, Cut, Find, FindNext, FindPrev,
    FontDecrease, FontIncrease, FontReset, NewConnection, NewTerminal, NextTab, OpenSettings,
    PaletteAiSidebar, PaletteBroadcast, PaletteCancelReconnect, PaletteCleanupDead,
    PaletteDetachTerminal, PaletteDisconnectAll, PaletteEventLog, PaletteHealthCheck,
    PaletteReconnectAll, PaletteResetPanes, Paste, PrevTab, Quit, ShellLauncher, ShowShortcuts,
    SplitHorizontal, SplitVertical, TerminalRecording, ToggleSidebar, ZenMode,
};

#[cfg(target_os = "macos")]
gpui::actions!(oxideterm, [Hide, HideOthers, Minimize, MinimizeAll]);

#[cfg(target_os = "macos")]
pub(crate) fn install_window_actions(cx: &mut gpui::App) {
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &Minimize, cx| {
        if let Some(handle) = cx.active_window() {
            // Key dispatch already borrows the current window. Update it after dispatch ends.
            cx.defer(move |cx| {
                // The window may have closed before this deferred action runs.
                let _ = handle.update(cx, |_, window, _| window.minimize_window());
            });
        }
    });
    cx.on_action(|_: &MinimizeAll, cx| {
        let windows = cx.windows();
        cx.defer(move |cx| {
            for handle in windows {
                let _ = handle.update(cx, |_, window, _| window.minimize_window());
            }
        });
    });
}

pub(crate) fn app_menus(settings: &PersistedSettings) -> Vec<Menu> {
    let i18n = I18n::new(crate::workspace::locale_from_settings(
        settings.general.language,
    ));
    vec![
        Menu {
            disabled: false,
            name: i18n.t("menu.app").into(),
            items: vec![
                MenuItem::os_submenu(i18n.t("menu.services"), SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(i18n.t("command_palette.title"), CommandPalette),
                MenuItem::action(i18n.t("menu.settings"), OpenSettings),
                MenuItem::action(i18n.t("command_palette.cmd_show_shortcuts"), ShowShortcuts),
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::action(i18n.t("settings_view.keybindings.actions.app.hide"), Hide),
                #[cfg(target_os = "macos")]
                MenuItem::action(
                    i18n.t("settings_view.keybindings.actions.app.hideOthers"),
                    HideOthers,
                ),
                #[cfg(target_os = "macos")]
                MenuItem::separator(),
                MenuItem::action(i18n.t("menu.quit"), Quit),
            ],
        },
        Menu {
            disabled: false,
            name: i18n.t("menu.edit").into(),
            items: vec![
                MenuItem::action(i18n.t("menu.cut"), Cut),
                MenuItem::action(i18n.t("menu.copy"), Copy),
                MenuItem::action(i18n.t("menu.paste"), Paste),
                MenuItem::separator(),
                MenuItem::action(i18n.t("menu.find"), Find),
                MenuItem::action(i18n.t("menu.find_next"), FindNext),
                MenuItem::action(i18n.t("menu.find_previous"), FindPrev),
            ],
        },
        Menu {
            disabled: false,
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
            disabled: false,
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
            disabled: false,
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
            disabled: false,
            name: i18n.t("menu.window").into(),
            items: vec![
                #[cfg(target_os = "macos")]
                MenuItem::action(
                    i18n.t("settings_view.keybindings.actions.app.minimize"),
                    Minimize,
                ),
                #[cfg(target_os = "macos")]
                MenuItem::action(
                    i18n.t("settings_view.keybindings.actions.app.minimizeAll"),
                    MinimizeAll,
                ),
                #[cfg(target_os = "macos")]
                MenuItem::separator(),
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
    ]
}

pub(crate) fn app_key_bindings(settings: &PersistedSettings) -> Vec<KeyBinding> {
    crate::keybindings::startup_key_bindings(&settings.keybindings.overrides)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_settings::Language;

    #[test]
    fn native_menu_labels_follow_saved_language() {
        let mut settings = PersistedSettings::default();
        for (language, edit, terminal) in [
            (Language::En, "Edit", "Terminal"),
            (Language::ZhCn, "编辑", "终端"),
        ] {
            settings.general.language = language;
            let menus = app_menus(&settings);
            assert_eq!(menus[1].name.as_ref(), edit);
            assert_eq!(menus[2].name.as_ref(), terminal);
            #[cfg(target_os = "macos")]
            {
                let expected = if language == Language::En {
                    [
                        "Hide OxideTerm",
                        "Hide Other Apps",
                        "Minimize Window",
                        "Minimize All Windows",
                    ]
                } else {
                    [
                        "隐藏 OxideTerm",
                        "隐藏其他应用",
                        "最小化当前窗口",
                        "最小化所有窗口",
                    ]
                };
                let actions: [Box<dyn gpui::Action>; 4] = [
                    Box::new(Hide),
                    Box::new(HideOthers),
                    Box::new(Minimize),
                    Box::new(MinimizeAll),
                ];
                for (index, (action, label)) in actions.iter().zip(expected).enumerate() {
                    let menu = if index < 2 {
                        &menus[0]
                    } else {
                        menus.last().unwrap()
                    };
                    let names = menu
                        .items
                        .iter()
                        .filter_map(|item| match item {
                            MenuItem::Action {
                                name,
                                action: candidate,
                                ..
                            } if candidate.partial_eq(action.as_ref()) => Some(name.as_ref()),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(names, vec![label]);
                }
            }
        }
    }
}
