// The native GPUI app is a Windows GUI process. Without this subsystem flag,
// Windows launches a console host for the installed app and closing that
// console also terminates OxideTerm.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app_icon;
mod assets;
mod bundled_fonts;
mod keybindings;
mod logging;
mod migration_snapshot;
mod platform;
mod single_instance;
mod workspace;

use std::path::PathBuf;

use gpui::{App, AppContext, Bounds, actions, px, size};
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

    // Match Tauri's startup ordering: portable detection and instance handling
    // happen before any settings or connection stores choose their data path.
    if let Err(error) = oxideterm_portable_runtime::initialize_portable_runtime() {
        eprintln!("failed to initialize OxideTerm portable runtime: {error}");
        std::process::exit(1);
    }
    let single_instance = single_instance::acquire_or_forward(ssh_launch_path.clone())
        .unwrap_or_else(|error| {
            eprintln!("failed to initialize OxideTerm single-instance guard: {error}");
            std::process::exit(1);
        });
    let single_instance::SingleInstanceOutcome::Primary {
        _guard: _single_instance_guard,
        receiver: single_instance_rx,
    } = single_instance
    else {
        return;
    };
    if let Err(error) = oxideterm_portable_runtime::acquire_portable_instance_lock() {
        eprintln!("failed to initialize OxideTerm portable runtime: {error}");
        std::process::exit(1);
    }
    // Only the primary process may snapshot mutable stores. This still runs
    // before SettingsStore or ConnectionStore can perform migrations.
    let settings_path = oxideterm_settings::default_settings_path();
    if let Err(error) = migration_snapshot::ensure_pre_2_0_migration_snapshot(&settings_path) {
        eprintln!("failed to create the pre-2.0 migration snapshot: {error:#}");
        std::process::exit(1);
    }
    let ssh_launch =
        single_instance::read_ssh_launch_file(ssh_launch_path).unwrap_or_else(|error| {
            eprintln!("failed to read SSH launch request: {error}");
            std::process::exit(2);
        });
    let startup_settings_store = SettingsStore::load_default();
    let startup_settings = startup_settings_store
        .as_ref()
        .map(|store| store.settings().clone())
        .unwrap_or_default();
    let _log_guard = match logging::init_file_logging(
        &startup_settings,
        startup_settings_store
            .as_ref()
            .ok()
            .map(SettingsStore::path),
    ) {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("failed to initialize OxideTerm file logging: {error:#}");
            None
        }
    };

    let application = oxideterm_gpui_platform::application().with_assets(NativeAssets);
    let reopen_single_instance_rx = single_instance_rx.clone();
    application.on_reopen(move |cx| {
        if !cx.windows().is_empty() {
            oxideterm_desktop_presence::show_main_window();
            return;
        }

        // macOS keeps the application alive after closing the last window.
        // Reopening from the Dock should create a fresh workspace window
        // instead of leaving the app windowless.
        if let Err(error) = open_main_workspace_window(
            cx,
            None,
            desktop_presence_menu_from_settings(),
            Some(reopen_single_instance_rx.clone()),
        ) {
            eprintln!(
                "OxideTerm could not reopen a native GPUI window: {error:#}\n\
                 Try updating GPU drivers, disabling incompatible graphics layers, \
                 or relaunching with OXIDETERM_RENDER_PROFILE=compatibility."
            );
        }
    });

    application.run(move |cx: &mut App| {
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
        cx.activate(true);
        cx.on_action(quit);
        cx.bind_keys(platform::app_key_bindings(&startup_settings));
        cx.set_menus(platform::app_menus(&I18n::default()));

        let desktop_presence_menu = desktop_presence_menu(&I18n::new(locale_from_settings(
            startup_settings.general.language,
        )));
        if let Err(err) = open_main_workspace_window(
            cx,
            ssh_launch,
            desktop_presence_menu,
            Some(single_instance_rx),
        ) {
            eprintln!(
                "OxideTerm could not open a native GPUI window: {err:#}\n\
                 GPUI 0.2.2 does not expose a CPU renderer fallback. \
                 Try updating GPU drivers, disabling incompatible graphics layers, \
                 or relaunching with OXIDETERM_RENDER_PROFILE=compatibility."
            );
            cx.quit();
            return;
        }

        #[cfg(target_os = "windows")]
        if let Err(error) = confirm_windows_update_after_initial_workspace() {
            eprintln!("failed to confirm the applied Windows update: {error}");
        }
    });
}

#[cfg(target_os = "windows")]
fn confirm_windows_update_after_initial_workspace() -> std::io::Result<()> {
    // Reaching this point confirms window and workspace construction. The old
    // files are recovery artifacts only and can now be removed without rollback.
    let current_exe = std::env::current_exe()?;
    let install_dir = current_exe.parent().ok_or_else(|| {
        std::io::Error::other(format!(
            "current executable has no install directory: {}",
            current_exe.display()
        ))
    })?;
    oxideterm_update::confirm_applied_windows_update(install_dir)
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
    single_instance_rx: Option<single_instance::SingleInstanceReceiver>,
) -> anyhow::Result<()> {
    let bounds = default_window_bounds(cx);
    cx.open_window(platform::window_options(bounds), |window, cx| {
        let desktop_presence_rx =
            match oxideterm_desktop_presence::install_for_window(window, cx, desktop_presence_menu)
            {
                Ok(rx) => rx,
                Err(error) => {
                    eprintln!(
                        "failed to install OxideTerm desktop presence integration: {error:#}"
                    );
                    None
                }
            };

        let workspace = cx.new(|cx| {
            WorkspaceApp::new(window, cx, desktop_presence_rx, single_instance_rx).unwrap_or_else(
                |err| {
                    panic!(
                        "failed to initialize OxideTerm workspace: {err:#}\n\
                     OxideTerm native uses GPUI's GPU-backed renderer. \
                     To retry with lightweight visual effects, launch with \
                     OXIDETERM_RENDER_PROFILE=compatibility."
                    )
                },
            )
        });
        let _ = workspace.update(cx, |workspace, cx| {
            workspace.start_desktop_presence_polling(cx);
            workspace.start_single_instance_polling(window, cx);
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

fn quit(_: &Quit, cx: &mut App) {
    oxideterm_desktop_presence::request_quit();
    cx.quit();
}

fn desktop_presence_menu(i18n: &I18n) -> oxideterm_desktop_presence::DesktopPresenceMenu {
    oxideterm_desktop_presence::DesktopPresenceMenu {
        app_name: i18n.t("menu.app"),
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
