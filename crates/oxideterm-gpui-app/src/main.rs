// The native GPUI app is a Windows GUI process. Without this subsystem flag,
// Windows launches a console host for the installed app and closing that
// console also terminates OxideTerm.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app_icon;
mod assets;
mod bundled_fonts;
mod keybindings;
mod platform;
mod workspace;

use std::time::Duration;

use gpui::{App, AppContext, Application, Bounds, Timer, actions, px, size};
use gpui_component::Root;
use oxideterm_i18n::I18n;
use oxideterm_settings::SettingsStore;

use crate::assets::NativeAssets;
use crate::workspace::WorkspaceApp;

// Tauri's `tauri.conf.json` opens the main window at 1200x800. Keeping the
// native default the same preserves first-launch sidebar proportions.
const TAURI_DEFAULT_WINDOW_WIDTH: f32 = 1200.0;
const TAURI_DEFAULT_WINDOW_HEIGHT: f32 = 800.0;

actions!(
    oxideterm,
    [
        Quit,
        NewTerminal,
        ShellLauncher,
        CloseTab,
        CloseOtherTabs,
        NewConnection,
        ToggleSidebar,
        CommandPalette,
        ZenMode,
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
        FontIncrease,
        FontDecrease,
        FontReset,
        ShowShortcuts,
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
        ClosePane,
        SplitNavLeft,
        SplitNavRight,
        TerminalAiPanel,
        TerminalRecording,
        PaletteEventLog,
        PaletteAiSidebar,
        PaletteBroadcast,
        PaletteDisconnectAll,
        PaletteReconnectAll,
        PaletteCancelReconnect,
        PaletteHealthCheck,
        PaletteResetPanes,
        PaletteDetachTerminal,
        PaletteCleanupDead
    ]
);

fn main() {
    // Match Tauri's startup ordering: portable detection and instance locking
    // happen before any settings or connection stores choose their data path.
    if let Err(error) = oxideterm_portable_runtime::initialize_portable_runtime()
        .and_then(|_| oxideterm_portable_runtime::acquire_portable_instance_lock())
    {
        eprintln!("failed to initialize OxideTerm portable runtime: {error}");
        std::process::exit(1);
    }

    Application::new()
        .with_assets(NativeAssets)
        .run(|cx: &mut App| {
            app_icon::install_runtime_app_icon();

            let startup_settings = SettingsStore::load_default()
                .map(|store| store.settings().clone())
                .unwrap_or_default();
            if let Err(error) =
                bundled_fonts::load_terminal_font_open_critical(&startup_settings, &cx.text_system())
            {
                eprintln!(
                    "failed to load selected bundled terminal font; falling back to system fonts: {error}"
                );
            }
            let cjk_fallback_text_system = cx.text_system().clone();
            let foreground = cx.foreground_executor();
            foreground
                .spawn(async move {
                    // Mirrors Tauri's delayed CJK fallback warmup: keep window
                    // and terminal startup responsive, then register Maple
                    // Regular only.
                    Timer::after(Duration::from_millis(500)).await;
                    if let Err(error) = bundled_fonts::load_terminal_cjk_fallback_regular(
                        &cjk_fallback_text_system,
                    ) {
                        eprintln!(
                            "failed to load bundled CJK terminal fallback; falling back to system fonts: {error}"
                        );
                    }
                    Timer::after(Duration::from_millis(5_000)).await;
                    if let Err(error) =
                        bundled_fonts::load_terminal_cjk_secondary_faces(&cjk_fallback_text_system)
                    {
                        eprintln!(
                            "failed to load secondary bundled CJK terminal fonts; falling back to system fonts: {error}"
                        );
                    }
                })
                .detach();
            oxideterm_code_editor::init(cx);
            cx.activate(true);
            cx.on_action(quit);
            cx.bind_keys(platform::app_key_bindings(&startup_settings));
            cx.set_menus(platform::app_menus(&I18n::default()));

            let bounds = Bounds::centered(
                None,
                size(
                    px(TAURI_DEFAULT_WINDOW_WIDTH),
                    px(TAURI_DEFAULT_WINDOW_HEIGHT),
                ),
                cx,
            );

            if let Err(err) = cx.open_window(platform::window_options(bounds), |window, cx| {
                let workspace = cx.new(|cx| {
                    WorkspaceApp::new(window, cx).unwrap_or_else(|err| {
                        panic!(
                            "failed to initialize OxideTerm workspace: {err:#}\n\
                             OxideTerm native uses GPUI's GPU-backed renderer. \
                             To retry with lightweight visual effects, launch with \
                             OXIDETERM_RENDER_PROFILE=compatibility."
                        )
                    })
                });
                cx.new(|cx| Root::new(workspace, window, cx))
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
