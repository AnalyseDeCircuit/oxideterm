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

use std::{path::PathBuf, time::Duration};

use gpui::{App, AppContext, Application, Bounds, Timer, actions, px, size};
use oxideterm_i18n::I18n;
use oxideterm_settings::SettingsStore;

use crate::assets::NativeAssets;
use crate::workspace::{WorkspaceApp, locale_from_settings};

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
    oxideterm_acp_adapter::run_from_env_if_requested();
    let ssh_launch_path = ssh_launch_path_arg().unwrap_or_else(|error| {
        eprintln!("failed to read SSH launch argument: {error}");
        std::process::exit(2);
    });

    // Match Tauri's startup ordering: portable detection and instance locking
    // happen before any settings or connection stores choose their data path.
    if let Err(error) = oxideterm_portable_runtime::initialize_portable_runtime()
        .and_then(|_| oxideterm_portable_runtime::acquire_portable_instance_lock())
    {
        eprintln!("failed to initialize OxideTerm portable runtime: {error}");
        std::process::exit(1);
    }
    let ssh_launch = read_ssh_launch_file(ssh_launch_path).unwrap_or_else(|error| {
        eprintln!("failed to read SSH launch request: {error}");
        std::process::exit(2);
    });

    let application = Application::new().with_assets(NativeAssets);
    application.on_reopen(|cx| {
        if !cx.windows().is_empty() {
            oxideterm_desktop_presence::show_main_window();
            return;
        }

        // macOS keeps the application alive after closing the last window.
        // Reopening from the Dock should create a fresh workspace window
        // instead of leaving the app windowless.
        if let Err(error) =
            open_main_workspace_window(cx, None, desktop_presence_menu_from_settings())
        {
            eprintln!(
                "OxideTerm could not reopen a native GPUI window: {error:#}\n\
                 Try updating GPU drivers, disabling incompatible graphics layers, \
                 or relaunching with OXIDETERM_RENDER_PROFILE=compatibility."
            );
        }
    });

    application.run(move |cx: &mut App| {
        let startup_settings = SettingsStore::load_default()
            .map(|store| store.settings().clone())
            .unwrap_or_default();
        oxideterm_desktop_presence::set_keep_running_on_close(
            startup_settings.general.minimize_to_tray_on_close,
        );
        app_icon::install_runtime_app_icon(startup_settings.appearance.app_icon);
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
                if let Err(error) =
                    bundled_fonts::load_terminal_cjk_fallback_regular(&cjk_fallback_text_system)
                {
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
        cx.activate(true);
        cx.on_action(quit);
        cx.bind_keys(platform::app_key_bindings(&startup_settings));
        cx.set_menus(platform::app_menus(&I18n::default()));

        let desktop_presence_menu = desktop_presence_menu(&I18n::new(locale_from_settings(
            startup_settings.general.language,
        )));
        if let Err(err) = open_main_workspace_window(cx, ssh_launch, desktop_presence_menu) {
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

fn default_window_bounds(cx: &mut App) -> Bounds<gpui::Pixels> {
    Bounds::centered(
        None,
        size(
            px(TAURI_DEFAULT_WINDOW_WIDTH),
            px(TAURI_DEFAULT_WINDOW_HEIGHT),
        ),
        cx,
    )
}

fn open_main_workspace_window(
    cx: &mut App,
    ssh_launch: Option<oxideterm_ssh_launch::TemporarySshLaunch>,
    desktop_presence_menu: oxideterm_desktop_presence::DesktopPresenceMenu,
) -> anyhow::Result<()> {
    let bounds = default_window_bounds(cx);
    cx.open_window(platform::window_options(bounds), |window, cx| {
        let desktop_presence_rx =
            match oxideterm_desktop_presence::install_for_window(window, cx, desktop_presence_menu)
            {
                Ok(rx) => Some(rx),
                Err(error) => {
                    eprintln!(
                        "failed to install OxideTerm desktop presence integration: {error:#}"
                    );
                    None
                }
            };

        let workspace = cx.new(|cx| {
            WorkspaceApp::new(window, cx, desktop_presence_rx).unwrap_or_else(|err| {
                panic!(
                    "failed to initialize OxideTerm workspace: {err:#}\n\
                     OxideTerm native uses GPUI's GPU-backed renderer. \
                     To retry with lightweight visual effects, launch with \
                     OXIDETERM_RENDER_PROFILE=compatibility."
                )
            })
        });
        let _ = workspace.update(cx, |workspace, cx| {
            workspace.start_desktop_presence_polling(cx);
        });
        if let Some(launch) = ssh_launch
            && let Err(error) = workspace.update(cx, |workspace, cx| {
                workspace.open_temporary_ssh_launch(launch, window, cx)
            })
        {
            eprintln!("failed to open temporary SSH launch: {error}");
        }
        workspace
    })
    .map(|_| ())
}

fn ssh_launch_path_arg() -> Result<Option<PathBuf>, String> {
    let mut args = std::env::args_os();
    let _program = args.next();
    while let Some(arg) = args.next() {
        if arg == "--ssh-launch-file" {
            return args
                .next()
                .map(PathBuf::from)
                .map(Some)
                .ok_or_else(|| "--ssh-launch-file requires a path".to_string());
        }
    }
    Ok(None)
}

fn read_ssh_launch_file(
    path: Option<PathBuf>,
) -> Result<Option<oxideterm_ssh_launch::TemporarySshLaunch>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    // The CLI handoff file may contain a stdin password. Delete it only after
    // the app owns the single-instance lock, otherwise a second process would
    // discard a request that it cannot open.
    let _ = std::fs::remove_file(&path);
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| format!("invalid SSH launch request: {error}"))
}

fn quit(_: &Quit, cx: &mut App) {
    oxideterm_desktop_presence::request_quit();
    cx.quit();
}

fn desktop_presence_menu(i18n: &I18n) -> oxideterm_desktop_presence::DesktopPresenceMenu {
    oxideterm_desktop_presence::DesktopPresenceMenu {
        app_name: i18n.t("menu.app"),
        status_title: "Ox".to_string(),
        status_icon: Some(oxideterm_desktop_presence::DesktopPresenceIcon {
            // macOS menu bar extras use the alpha channel of a dedicated
            // template image; the full-color app icon is not a good source.
            template_png_bytes: include_bytes!("../resources/icons/menu-bar-template@2x.png"),
            point_size: 18.0,
        }),
        show_main_window: i18n.t("menu.show_main_window"),
        hide_main_window: i18n.t("menu.hide_main_window"),
        new_connection: i18n.t("layout.empty.new_connection"),
        settings: i18n.t("menu.settings"),
        check_for_updates: i18n.t("settings_view.help.check_update"),
        quit: i18n.t("menu.quit"),
    }
}

fn desktop_presence_menu_from_settings() -> oxideterm_desktop_presence::DesktopPresenceMenu {
    let settings = SettingsStore::load_default()
        .map(|store| store.settings().clone())
        .unwrap_or_default();
    desktop_presence_menu(&I18n::new(locale_from_settings(settings.general.language)))
}
