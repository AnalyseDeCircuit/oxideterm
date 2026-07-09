use std::{
    mem::size_of,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering},
        mpsc,
    },
    thread,
};

use anyhow::{Context as _, anyhow};
use gpui::{App, Window};
use raw_window_handle::RawWindowHandle;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION, NIN_SELECT,
                NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CS_HREDRAW, CS_VREDRAW, CreatePopupMenu, CreateWindowExW,
                DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos,
                GetMessageW, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED, LoadImageW, MF_SEPARATOR,
                MF_STRING, MSG, PostMessageW, PostQuitMessage, RegisterClassW,
                RegisterWindowMessageW, SW_HIDE, SW_RESTORE, SW_SHOW, SetForegroundWindow,
                ShowWindowAsync, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
                TRACK_POPUP_MENU_FLAGS, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP,
                WM_CONTEXTMENU, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
                WNDCLASSW, WS_OVERLAPPED,
            },
        },
    },
    core::{PCWSTR, w},
};

use crate::{DesktopPresenceEvent, DesktopPresenceMenu};

const TRAY_ICON_ID: u32 = 1;
const TRAY_CALLBACK_MESSAGE: u32 = WM_APP + 0x51;
const TRAY_SHUTDOWN_MESSAGE: u32 = WM_APP + 0x52;
const TRAY_NIN_KEYSELECT: u32 = NIN_SELECT + 1;
const TRAY_MENU_SHOW: u32 = 1001;
const TRAY_MENU_HIDE: u32 = 1002;
const TRAY_MENU_NEW_CONNECTION: u32 = 1003;
const TRAY_MENU_SETTINGS: u32 = 1004;
const TRAY_MENU_CHECK_UPDATES: u32 = 1005;
const TRAY_MENU_QUIT: u32 = 1006;

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED_MESSAGE: AtomicU32 = AtomicU32::new(0);
static TRAY_THREAD_STARTED: AtomicBool = AtomicBool::new(false);
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static KEEP_RUNNING_ON_CLOSE: AtomicBool = AtomicBool::new(true);
static EVENT_TX: OnceLock<Mutex<Option<mpsc::Sender<DesktopPresenceEvent>>>> = OnceLock::new();
static MENU: OnceLock<Mutex<DesktopPresenceMenu>> = OnceLock::new();

pub(crate) fn install_for_window(
    window: &mut Window,
    cx: &App,
    menu: DesktopPresenceMenu,
    tx: mpsc::Sender<DesktopPresenceEvent>,
) -> anyhow::Result<()> {
    let hwnd = main_window_hwnd(window)?;
    MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    set_menu(menu);
    set_event_sender(tx);
    start_tray_thread_once()?;

    window.on_window_should_close(cx, move |_window, _cx| {
        if QUIT_REQUESTED.load(Ordering::SeqCst) || !KEEP_RUNNING_ON_CLOSE.load(Ordering::SeqCst) {
            return true;
        }

        // Close-to-background is visual-only; the explicit Quit action still
        // asks GPUI to terminate and lets the shell icon clean itself up.
        hide_hwnd(hwnd);
        false
    });

    Ok(())
}

pub(crate) fn set_keep_running_on_close(enabled: bool) {
    KEEP_RUNNING_ON_CLOSE.store(enabled, Ordering::SeqCst);
}

pub(crate) fn show_main_window() {
    let hwnd = HWND(MAIN_HWND.load(Ordering::SeqCst) as _);
    if hwnd.is_invalid() {
        return;
    }
    unsafe {
        let _ = ShowWindowAsync(hwnd, SW_SHOW);
        let _ = ShowWindowAsync(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
}

pub(crate) fn hide_main_window() {
    let hwnd = HWND(MAIN_HWND.load(Ordering::SeqCst) as _);
    if !hwnd.is_invalid() {
        hide_hwnd(hwnd);
    }
}

pub(crate) fn request_quit() {
    QUIT_REQUESTED.store(true, Ordering::SeqCst);
    let tray_hwnd = HWND(TRAY_HWND.load(Ordering::SeqCst) as _);
    if !tray_hwnd.is_invalid() {
        remove_tray_icon(tray_hwnd);
        unsafe {
            let _ = PostMessageW(Some(tray_hwnd), TRAY_SHUTDOWN_MESSAGE, WPARAM(0), LPARAM(0));
        }
    }
}

fn set_event_sender(tx: mpsc::Sender<DesktopPresenceEvent>) {
    let mut sender = EVENT_TX
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("desktop presence event sender poisoned");
    *sender = Some(tx);
}

fn set_menu(menu: DesktopPresenceMenu) {
    let mut stored = MENU
        .get_or_init(|| Mutex::new(DesktopPresenceMenu::fallback()))
        .lock()
        .expect("desktop presence menu poisoned");
    *stored = menu;
}

fn current_menu() -> DesktopPresenceMenu {
    MENU.get_or_init(|| Mutex::new(DesktopPresenceMenu::fallback()))
        .lock()
        .expect("desktop presence menu poisoned")
        .clone()
}

fn send_event(event: DesktopPresenceEvent) {
    if let Some(tx) = EVENT_TX
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("desktop presence event sender poisoned")
        .as_ref()
    {
        let _ = tx.send(event);
    }
}

fn main_window_hwnd(window: &Window) -> anyhow::Result<HWND> {
    // GPUI also exposes Window::window_handle() with a different return type,
    // so call the raw-window-handle trait explicitly here.
    let handle = raw_window_handle::HasWindowHandle::window_handle(window)
        .map_err(|_| anyhow!("unable to read Windows window handle"))?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err(anyhow!("OxideTerm main window is not a Win32 window"));
    };
    Ok(HWND(handle.hwnd.get() as _))
}

fn start_tray_thread_once() -> anyhow::Result<()> {
    if TRAY_THREAD_STARTED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    thread::Builder::new()
        .name("oxideterm-windows-tray".to_string())
        .spawn(|| {
            if let Err(error) = run_tray_message_loop() {
                eprintln!("failed to start OxideTerm Windows tray icon: {error:#}");
            }
        })
        .context("failed to spawn Windows tray thread")?;
    Ok(())
}

fn run_tray_message_loop() -> anyhow::Result<()> {
    let module = unsafe { GetModuleHandleW(None).context("unable to get module handle")? };
    let taskbar_created_message = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    if taskbar_created_message == 0 {
        return Err(anyhow!("failed to register TaskbarCreated message"));
    }
    TASKBAR_CREATED_MESSAGE.store(taskbar_created_message, Ordering::SeqCst);
    let class_name = w!("OxideTermTrayWindow");
    let window_class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(tray_window_proc),
        hInstance: module.into(),
        lpszClassName: class_name,
        ..Default::default()
    };

    unsafe {
        RegisterClassW(&window_class);
    }

    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!(""),
            WINDOW_STYLE(WS_OVERLAPPED.0),
            0,
            0,
            0,
            0,
            // TaskbarCreated is broadcast only to top-level windows. Keep this
            // window hidden, but do not make it a message-only HWND.
            None,
            None,
            Some(module.into()),
            None,
        )
        .context("failed to create tray message window")?
    };
    TRAY_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    add_tray_icon(hwnd).context("failed to add tray icon")?;

    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, None, 0, 0).as_bool() } {
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    TRAY_HWND.store(0, Ordering::SeqCst);
    Ok(())
}

unsafe extern "system" fn tray_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if is_taskbar_created_message(message, TASKBAR_CREATED_MESSAGE.load(Ordering::SeqCst)) {
        // Explorer removes notification icons when its taskbar restarts. Add
        // the existing icon again and restore the versioned callback contract.
        if let Err(error) = add_tray_icon(hwnd) {
            eprintln!("failed to restore OxideTerm tray icon: {error:#}");
        }
        return LRESULT(0);
    }

    match message {
        TRAY_CALLBACK_MESSAGE => {
            match tray_notification_code(lparam) {
                NIN_SELECT | TRAY_NIN_KEYSELECT | WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                    send_event(DesktopPresenceEvent::ShowMainWindow)
                }
                WM_RBUTTONUP | WM_CONTEXTMENU => show_tray_menu(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        TRAY_SHUTDOWN_MESSAGE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            remove_tray_icon(hwnd);
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn tray_notification_code(lparam: LPARAM) -> u32 {
    // NOTIFYICON_VERSION_4 can pack extra data into the high word of lParam.
    // Routing by the low-word notification code keeps both modern NIN_* and
    // older mouse-message callbacks working.
    (lparam.0 as u32) & 0xffff
}

fn is_taskbar_created_message(message: u32, registered_message: u32) -> bool {
    // Registration failure returns zero, which must never classify a regular
    // window message as an Explorer restart notification.
    registered_message != 0 && message == registered_message
}

fn add_tray_icon(hwnd: HWND) -> anyhow::Result<()> {
    let mut data = base_notify_icon_data(hwnd);
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = TRAY_CALLBACK_MESSAGE;
    data.hIcon = load_app_icon().context("failed to load tray icon resource")?;
    set_tip(&mut data, &current_menu().app_name);

    unsafe {
        Shell_NotifyIconW(NIM_ADD, &data)
            .as_bool()
            .then_some(())
            .ok_or_else(|| anyhow!("Shell_NotifyIconW(NIM_ADD) failed"))?;
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &data)
            .as_bool()
            .then_some(())
            .ok_or_else(|| anyhow!("Shell_NotifyIconW(NIM_SETVERSION) failed"))?;
    }
    Ok(())
}

fn remove_tray_icon(hwnd: HWND) {
    let data = base_notify_icon_data(hwnd);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &data);
    }
}

fn base_notify_icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_ID,
        ..Default::default()
    }
}

fn load_app_icon() -> anyhow::Result<windows::Win32::UI::WindowsAndMessaging::HICON> {
    let module = unsafe { GetModuleHandleW(None).context("unable to get module handle")? };
    let handle = unsafe {
        LoadImageW(
            Some(module.into()),
            PCWSTR(1 as _),
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE | LR_SHARED,
        )
        .context("unable to load embedded icon")?
    };
    Ok(windows::Win32::UI::WindowsAndMessaging::HICON(handle.0))
}

fn set_tip(data: &mut NOTIFYICONDATAW, tip: &str) {
    for (slot, value) in data.szTip.iter_mut().zip(tip.encode_utf16()) {
        *slot = value;
    }
}

fn show_tray_menu(hwnd: HWND) {
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else {
        return;
    };
    let labels = current_menu();

    unsafe {
        append_menu_item(menu, TRAY_MENU_SHOW, &labels.show_main_window);
        append_menu_item(menu, TRAY_MENU_HIDE, &labels.hide_main_window);
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        append_menu_item(menu, TRAY_MENU_NEW_CONNECTION, &labels.new_connection);
        append_menu_item(menu, TRAY_MENU_SETTINGS, &labels.settings);
        append_menu_item(menu, TRAY_MENU_CHECK_UPDATES, &labels.check_for_updates);
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        append_menu_item(menu, TRAY_MENU_QUIT, &labels.quit);

        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_ok() {
            let _ = SetForegroundWindow(hwnd);
            let command = windows::Win32::UI::WindowsAndMessaging::TrackPopupMenu(
                menu,
                TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY | TRACK_POPUP_MENU_FLAGS(0),
                point.x,
                point.y,
                None,
                hwnd,
                None,
            )
            .0 as u32;

            match command {
                TRAY_MENU_SHOW => send_event(DesktopPresenceEvent::ShowMainWindow),
                TRAY_MENU_HIDE => send_event(DesktopPresenceEvent::HideMainWindow),
                TRAY_MENU_NEW_CONNECTION => send_event(DesktopPresenceEvent::NewConnection),
                TRAY_MENU_SETTINGS => send_event(DesktopPresenceEvent::OpenSettings),
                TRAY_MENU_CHECK_UPDATES => send_event(DesktopPresenceEvent::CheckForUpdates),
                TRAY_MENU_QUIT => send_event(DesktopPresenceEvent::Quit),
                _ => {}
            }
            let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        }
        let _ = DestroyMenu(menu);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_notification_code_uses_low_word_only() {
        let packed_lparam = LPARAM(((0x4321_u32 << 16) | WM_CONTEXTMENU) as isize);

        assert_eq!(tray_notification_code(packed_lparam), WM_CONTEXTMENU);
    }

    #[test]
    fn taskbar_created_message_requires_registered_match() {
        const REGISTERED_MESSAGE: u32 = 0xc123;

        assert!(is_taskbar_created_message(
            REGISTERED_MESSAGE,
            REGISTERED_MESSAGE
        ));
        assert!(!is_taskbar_created_message(0, 0));
        assert!(!is_taskbar_created_message(
            REGISTERED_MESSAGE + 1,
            REGISTERED_MESSAGE
        ));
    }
}

unsafe fn append_menu_item(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    id: u32,
    label: &str,
) {
    let wide = label
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        let _ = AppendMenuW(menu, MF_STRING, id as usize, PCWSTR(wide.as_ptr()));
    }
}

fn hide_hwnd(hwnd: HWND) {
    unsafe {
        let _ = ShowWindowAsync(hwnd, SW_HIDE);
    }
}
