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
        SwitchLocaleEnglish,
        SwitchLocaleChinese,
        SplitHorizontal,
        SplitVertical,
        ClosePane
    ]
);

fn main() {
    Application::new()
        .with_assets(NativeAssets)
        .run(|cx: &mut App| {
            cx.activate(true);
            cx.on_action(quit);
            cx.bind_keys(platform::app_key_bindings());
            cx.set_menus(platform::app_menus(&I18n::default()));

            let bounds = Bounds::centered(None, size(px(1120.0), px(760.0)), cx);

            cx.open_window(platform::window_options(bounds), |window, cx| {
                cx.new(|cx| WorkspaceApp::new(window, cx).expect("failed to initialize workspace"))
            })
            .expect("failed to open OxideTerm native window");
        });
}

fn quit(_: &Quit, cx: &mut App) {
    cx.quit();
}
