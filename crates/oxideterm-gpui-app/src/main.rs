mod assets;
mod platform;
mod workspace;

use gpui::{App, AppContext, Application, Bounds, actions, px, size};
use oxideterm_i18n::I18n;

use crate::assets::NativeAssets;
use crate::workspace::WorkspaceApp;

actions!(
    oxideterm,
    [
        Quit,
        NewTerminal,
        CloseTab,
        NextTab,
        PrevTab,
        GoToTab1,
        GoToTab2,
        GoToTab3,
        GoToTab4,
        GoToTab5,
        GoToTab6,
        GoToTab7,
        GoToTab8,
        GoToTab9,
        Copy,
        Paste,
        Find,
        FindNext,
        FindPrev,
        CloseSearch,
        OpenSettings,
        SwitchLocaleEnglish,
        SwitchLocaleChinese,
        SwitchLocaleTraditionalChinese,
        SwitchLocaleGerman,
        SwitchLocaleSpanish,
        SwitchLocaleFrench,
        SwitchLocaleItalian,
        SwitchLocaleJapanese,
        SwitchLocaleKorean,
        SwitchLocalePortugueseBrazil,
        SwitchLocaleVietnamese,
        SplitHorizontal,
        SplitVertical,
        ClosePane
    ]
);

fn main() {
    Application::new()
        .with_assets(NativeAssets)
        .run(|cx: &mut App| {
            oxideterm_code_editor::init(cx);
            cx.activate(true);
            cx.on_action(quit);
            cx.bind_keys(platform::app_key_bindings());
            cx.set_menus(platform::app_menus(&I18n::default()));

            let bounds = Bounds::centered(None, size(px(1120.0), px(760.0)), cx);

            if let Err(err) = cx.open_window(platform::window_options(bounds), |window, cx| {
                cx.new(|cx| {
                    WorkspaceApp::new(window, cx).unwrap_or_else(|err| {
                        panic!(
                            "failed to initialize OxideTerm workspace: {err:#}\n\
                             OxideTerm native uses GPUI's GPU-backed renderer. \
                             To retry with lightweight visual effects, launch with \
                             OXIDETERM_RENDER_PROFILE=compatibility."
                        )
                    })
                })
            }) {
                eprintln!(
                    "OxideTerm could not open a native GPUI window: {err:#}\n\
                     GPUI 0.2.2 does not expose a CPU renderer fallback. \
                     Try updating GPU drivers, disabling incompatible graphics layers, \
                     or relaunching with OXIDETERM_RENDER_PROFILE=compatibility."
                );
                cx.quit();
            }
        });
}

fn quit(_: &Quit, cx: &mut App) {
    cx.quit();
}
