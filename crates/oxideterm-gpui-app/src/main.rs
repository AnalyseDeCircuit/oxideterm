mod assets;
mod keybindings;
mod platform;
mod workspace;

use gpui::{App, AppContext, Application, Bounds, actions, px, size};
use gpui_component::Root;
use oxideterm_i18n::I18n;
use oxideterm_settings::{SettingsStore, default_settings_path};
use std::{path::PathBuf, sync::OnceLock};

use crate::assets::NativeAssets;
use crate::workspace::WorkspaceApp;

static INSTANCE_LOCK: OnceLock<redb::Database> = OnceLock::new();

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
    if let Err(message) = acquire_native_instance_lock() {
        show_startup_error("OxideTerm 已在运行", &message);
        eprintln!("{message}");
        return;
    }

    if let Err(message) = check_native_data_store_locks() {
        show_startup_error("OxideTerm 已在运行", &message);
        eprintln!("{message}");
        return;
    }

    Application::new()
        .with_assets(NativeAssets)
        .run(|cx: &mut App| {
            oxideterm_code_editor::init(cx);
            cx.activate(true);
            cx.on_action(quit);
            let startup_settings = SettingsStore::load_default()
                .map(|store| store.settings().clone())
                .unwrap_or_default();
            cx.bind_keys(platform::app_key_bindings(&startup_settings));
            cx.set_menus(platform::app_menus(&I18n::default()));

            let bounds = Bounds::centered(None, size(px(1120.0), px(760.0)), cx);

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

fn acquire_native_instance_lock() -> Result<(), String> {
    let lock_path = native_data_dir().join("oxideterm-native-instance.redb");
    std::fs::create_dir_all(
        lock_path
            .parent()
            .ok_or_else(|| "无法确定 OxideTerm 数据目录".to_string())?,
    )
    .map_err(|error| format!("无法创建 OxideTerm 数据目录: {error}"))?;

    match redb::Database::create(&lock_path) {
        Ok(database) => {
            let _ = INSTANCE_LOCK.set(database);
            Ok(())
        }
        Err(error) if is_redb_lock_error(&error) => Err(format!(
            "另一个 OxideTerm 实例正在使用数据目录。\n\n请先关闭已打开的 OxideTerm 窗口，然后再重新启动。\n\n锁文件: {}",
            lock_path.display()
        )),
        Err(error) => Err(format!(
            "无法创建 OxideTerm 实例锁: {error}\n\n锁文件: {}",
            lock_path.display()
        )),
    }
}

fn check_native_data_store_locks() -> Result<(), String> {
    let data_dir = native_data_dir();
    for path in [
        data_dir.join("chat_history.redb"),
        data_dir.join("rag_index.redb"),
    ] {
        match redb::Database::create(&path) {
            Ok(_) => {}
            Err(error) if is_redb_lock_error(&error) => {
                return Err(format!(
                    "另一个 OxideTerm 实例正在使用数据目录。\n\n请先关闭已打开的 OxideTerm 窗口，然后再重新启动。\n\n正在使用的数据库: {}",
                    path.display()
                ));
            }
            Err(_) => {}
        }
    }
    Ok(())
}

fn native_data_dir() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn is_redb_lock_error(error: &redb::DatabaseError) -> bool {
    let message = error.to_string();
    message.contains("Database already open") || message.contains("Cannot acquire lock")
}

#[cfg(target_os = "macos")]
fn show_startup_error(title: &str, message: &str) {
    let script = format!(
        "display alert {} message {} as critical",
        osascript_string(title),
        osascript_string(message)
    );
    let _ = std::process::Command::new("osascript")
        .args(["-e", &script])
        .status();
}

#[cfg(target_os = "windows")]
fn show_startup_error(title: &str, message: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;

    let title: Vec<u16> = OsStr::new(title).encode_wide().chain(Some(0)).collect();
    let message: Vec<u16> = OsStr::new(message).encode_wide().chain(Some(0)).collect();

    unsafe {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBoxW(
                hwnd: *mut std::ffi::c_void,
                text: *const u16,
                caption: *const u16,
                type_: u32,
            ) -> i32;
        }
        MessageBoxW(null_mut(), message.as_ptr(), title.as_ptr(), 0x10);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn show_startup_error(_title: &str, _message: &str) {}

#[cfg(target_os = "macos")]
fn osascript_string(value: &str) -> String {
    format!("{value:?}")
}
