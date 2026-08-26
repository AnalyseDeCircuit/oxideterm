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
mod portable_bootstrap;
mod single_instance;
mod window_placement;
mod workspace;

use std::path::PathBuf;

use gpui::{App, AppContext, actions};
use oxideterm_i18n::I18n;
use oxideterm_settings::{SettingsStore, WindowUiState};
use zeroize::Zeroizing;

use crate::assets::NativeAssets;
use crate::window_placement::{default_window_bounds, initial_window_bounds};
use crate::workspace::{WorkspaceApp, WorkspaceWindowShell, locale_from_settings};

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
        ToggleFullscreen,
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
        Cut,
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
        TerminalClearScreen,
        TerminalRecording,
        TerminalFreeTypeMode,
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
    let native_launch_args = native_launch_args().unwrap_or_else(|error| {
        eprintln!("failed to read native connection launch argument: {error}");
        std::process::exit(2);
    });

    // Match Tauri's startup ordering: portable detection and instance handling
    // happen before any settings or connection stores choose their data path.
    if let Err(error) = oxideterm_portable_runtime::initialize_portable_runtime() {
        eprintln!("failed to initialize OxideTerm portable runtime: {error}");
        std::process::exit(1);
    }
    let single_instance = single_instance::acquire_or_forward(
        native_launch_args.handoff_path.clone(),
        native_launch_args.connection_launch,
    )
    .unwrap_or_else(|error| {
        eprintln!("failed to initialize OxideTerm single-instance guard: {error}");
        std::process::exit(1);
    });
    let single_instance::SingleInstanceOutcome::Primary {
        _guard: _single_instance_guard,
        receiver: single_instance_rx,
        startup_launch,
    } = single_instance
    else {
        return;
    };
    if let Err(error) = oxideterm_portable_runtime::acquire_portable_instance_lock() {
        eprintln!("failed to initialize OxideTerm portable runtime: {error}");
        std::process::exit(1);
    }
    if matches!(
        oxideterm_portable_runtime::portable_bootstrap_status(),
        Ok(oxideterm_portable_runtime::PortableBootstrapStatus::Locked)
    ) && oxideterm_portable_runtime::keystore::try_portable_auto_unlock().is_err()
    {
        // Automatic-unlock failures must retain the normal password fallback.
        // Avoid including backend details that may identify credential entries.
        eprintln!("portable automatic unlock is unavailable; password unlock is required");
    }
    // Only the primary process may snapshot mutable stores. This still runs
    // before SettingsStore or ConnectionStore can perform migrations.
    let settings_path = oxideterm_settings::default_settings_path();
    if let Err(error) = migration_snapshot::ensure_pre_2_0_migration_snapshot(&settings_path) {
        eprintln!("failed to create the pre-2.0 migration snapshot: {error:#}");
        std::process::exit(1);
    }
    let handoff_launch =
        single_instance::read_connection_launch_file(native_launch_args.handoff_path)
            .unwrap_or_else(|error| {
                eprintln!("failed to read connection launch request: {error}");
                std::process::exit(2);
            });
    let startup_settings_store = SettingsStore::load_default();
    let startup_settings = startup_settings_store
        .as_ref()
        .map(|store| store.settings().clone())
        .unwrap_or_default();
    if startup_launch.is_some()
        && handoff_launch.is_none()
        && !startup_settings.general.external_connection_uris_enabled
    {
        // A disabled external URI must not open an unrelated workspace window.
        return;
    }
    let native_connection_launch = handoff_launch.or_else(|| {
        startup_settings
            .general
            .external_connection_uris_enabled
            .then_some(startup_launch)
            .flatten()
    });
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
    let url_event_receiver = single_instance_rx.clone();
    application.on_open_urls(move |urls, cx| {
        let settings = SettingsStore::load_default()
            .map(|store| store.settings().clone())
            .unwrap_or_default();
        if !settings.general.external_connection_uris_enabled {
            return;
        }
        let default_username = whoami::username();
        let mut launches = Vec::new();
        for url in urls {
            let url = Zeroizing::new(url);
            match oxideterm_ssh_launch::parse_connection_uri(&url, Some(&default_username)) {
                Ok(launch) => launches.push(launch),
                Err(error) => eprintln!("failed to open connection URI: {error}"),
            }
        }
        if cx.windows().is_empty()
            && !launches.is_empty()
            && let Err(error) = open_primary_window(
                cx,
                Some(launches.remove(0)),
                desktop_presence_menu_from_settings(),
                Some(url_event_receiver.clone()),
                SettingsStore::load_default()
                    .map(|store| store.settings().clone())
                    .unwrap_or_default(),
            )
        {
            eprintln!("failed to open a window for connection URI: {error:#}");
            return;
        }
        for launch in launches {
            let _ = url_event_receiver
                .publish(single_instance::SingleInstanceEvent::OpenExternalConnectionUri(launch));
        }
    });
    let reopen_single_instance_rx = single_instance_rx.clone();
    application.on_reopen(move |cx| {
        if !cx.windows().is_empty() {
            oxideterm_desktop_presence::show_main_window();
            return;
        }

        // macOS keeps the application alive after closing the last window.
        // Reopening from the Dock should create a fresh workspace window
        // instead of leaving the app windowless.
        if let Err(error) = open_primary_window(
            cx,
            None,
            desktop_presence_menu_from_settings(),
            Some(reopen_single_instance_rx.clone()),
            SettingsStore::load_default()
                .map(|store| store.settings().clone())
                .unwrap_or_default(),
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
        #[cfg(target_os = "windows")]
        {
            // Keep Windows on the proven grayscale path until GPUI-CE subpixel repainting is stable.
            cx.set_text_rendering_mode(gpui::TextRenderingMode::Grayscale);
        }
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
        let workspace_opened = match open_primary_window(
            cx,
            native_connection_launch,
            desktop_presence_menu,
            Some(single_instance_rx),
            startup_settings.clone(),
        ) {
            Ok(workspace_opened) => workspace_opened,
            Err(err) => {
                eprintln!(
                    "OxideTerm could not open a native GPUI window: {err:#}\n\
                     GPUI 0.2.2 does not expose a CPU renderer fallback. \
                     Try updating GPU drivers, disabling incompatible graphics layers, \
                     or relaunching with OXIDETERM_RENDER_PROFILE=compatibility."
                );
                cx.quit();
                return;
            }
        };

        if workspace_opened && let Err(error) = confirm_update_after_initial_workspace() {
            eprintln!("failed to confirm the applied update: {error}");
        }
    });
}

fn confirm_update_after_initial_workspace() -> std::io::Result<()> {
    // Reaching this point confirms window and workspace construction. The old
    // files are recovery artifacts only and can now be removed without rollback.
    if let Ok(info) = oxideterm_portable_runtime::portable_info()
        && info.is_portable
    {
        oxideterm_update::confirm_applied_portable_update(&info.host_dir)?;
    }

    #[cfg(target_os = "windows")]
    {
        let current_exe = std::env::current_exe()?;
        let install_dir = current_exe.parent().ok_or_else(|| {
            std::io::Error::other(format!(
                "current executable has no install directory: {}",
                current_exe.display()
            ))
        })?;
        oxideterm_update::confirm_applied_windows_update(install_dir)?;
    }
    Ok(())
}

fn open_main_workspace_window(
    cx: &mut App,
    native_connection_launch: Option<oxideterm_ssh_launch::NativeConnectionLaunch>,
    desktop_presence_menu: oxideterm_desktop_presence::DesktopPresenceMenu,
    single_instance_rx: Option<single_instance::SingleInstanceReceiver>,
    window_ui: WindowUiState,
) -> anyhow::Result<()> {
    let window_bounds = initial_window_bounds(cx, &window_ui);
    cx.open_window(
        platform::window_options_with_bounds(window_bounds),
        |window, cx| {
            let desktop_presence_rx = match oxideterm_desktop_presence::install_for_window(
                window,
                cx,
                desktop_presence_menu,
            ) {
                Ok(rx) => rx,
                Err(error) => {
                    eprintln!(
                        "failed to install OxideTerm desktop presence integration: {error:#}"
                    );
                    None
                }
            };

            let session = cx.new(|cx| {
                WorkspaceApp::new(window, cx, desktop_presence_rx, single_instance_rx)
                    .unwrap_or_else(|err| {
                        panic!(
                            "failed to initialize OxideTerm workspace: {err:#}\n\
                     OxideTerm native uses GPUI's GPU-backed renderer. \
                     To retry with lightweight visual effects, launch with \
                     OXIDETERM_RENDER_PROFILE=compatibility."
                        )
                    })
            });
            if let Some(launch) = native_connection_launch
                && let Err(error) = session.update(cx, |session, cx| {
                    session.open_native_connection_launch(launch, window, cx)
                })
            {
                eprintln!("failed to open native connection launch: {error}");
            }
            cx.new(|cx| WorkspaceWindowShell::new(session, window, cx))
        },
    )
    .map(|_| ())
}

fn open_primary_window(
    cx: &mut App,
    native_connection_launch: Option<oxideterm_ssh_launch::NativeConnectionLaunch>,
    desktop_presence_menu: oxideterm_desktop_presence::DesktopPresenceMenu,
    single_instance_rx: Option<single_instance::SingleInstanceReceiver>,
    settings: oxideterm_settings::PersistedSettings,
) -> anyhow::Result<bool> {
    let portable_status = oxideterm_portable_runtime::portable_status_snapshot()?;
    if portable_bootstrap::portable_startup_requires_bootstrap(portable_status.status) {
        portable_bootstrap::open_portable_bootstrap_window(
            cx,
            portable_status,
            settings,
            native_connection_launch,
            desktop_presence_menu,
            single_instance_rx,
        )?;
        return Ok(false);
    }

    open_main_workspace_window(
        cx,
        native_connection_launch,
        desktop_presence_menu,
        single_instance_rx,
        settings.window_ui,
    )?;
    Ok(true)
}

struct NativeLaunchArgs {
    handoff_path: Option<PathBuf>,
    connection_launch: Option<oxideterm_ssh_launch::NativeConnectionLaunch>,
}

fn native_launch_args() -> Result<NativeLaunchArgs, String> {
    let mut args = std::env::args_os();
    let _program = args.next();
    let mut handoff_path = None;
    let mut connection_launch = None;
    let default_username = whoami::username();
    while let Some(arg) = args.next() {
        if arg == "--ssh-launch-file" || arg == "--connection-launch-file" {
            if handoff_path.is_some() || connection_launch.is_some() {
                return Err("only one native connection launch may be supplied".to_string());
            }
            handoff_path =
                Some(args.next().map(PathBuf::from).ok_or_else(|| {
                    "connection launch file argument requires a path".to_string()
                })?);
            continue;
        }
        let Ok(arg) = arg.into_string() else {
            continue;
        };
        if !looks_like_connection_uri(&arg) {
            continue;
        }
        if handoff_path.is_some() || connection_launch.is_some() {
            return Err("only one native connection launch may be supplied".to_string());
        }
        let arg = Zeroizing::new(arg);
        connection_launch = Some(
            oxideterm_ssh_launch::parse_connection_uri(&arg, Some(&default_username))
                .map_err(|error| error.to_string())?,
        );
    }
    Ok(NativeLaunchArgs {
        handoff_path,
        connection_launch,
    })
}

fn looks_like_connection_uri(value: &str) -> bool {
    value
        .trim_start()
        .split_once(':')
        .is_some_and(|(scheme, _)| {
            oxideterm_ssh_launch::SUPPORTED_CONNECTION_URI_SCHEMES
                .iter()
                .any(|candidate| scheme.eq_ignore_ascii_case(candidate))
        })
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
