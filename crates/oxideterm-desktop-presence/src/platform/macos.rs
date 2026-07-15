use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use anyhow::anyhow;
use gpui::{App, Window};
use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{NSApplication, NSView, NSWindow};
use raw_window_handle::RawWindowHandle;

use crate::{DesktopPresenceEvent, DesktopPresenceMenu};

static MAIN_WINDOW: AtomicUsize = AtomicUsize::new(0);
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static KEEP_RUNNING_ON_CLOSE: AtomicBool = AtomicBool::new(true);

pub(crate) fn install_for_window(
    window: &mut Window,
    cx: &App,
    _menu: DesktopPresenceMenu,
    _tx: std::sync::mpsc::Sender<DesktopPresenceEvent>,
) -> anyhow::Result<()> {
    let ns_window = main_window(window)?;
    MAIN_WINDOW.store(ns_window as usize, Ordering::SeqCst);

    window.on_window_should_close(cx, move |_window, _cx| {
        if QUIT_REQUESTED.load(Ordering::SeqCst) || !KEEP_RUNNING_ON_CLOSE.load(Ordering::SeqCst) {
            // Clear the non-owning pointer before GPUI releases the native window.
            let _ = MAIN_WINDOW.compare_exchange(
                ns_window as usize,
                0,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
            return true;
        }

        // Preserve active sessions by hiding the window. The Dock reopen
        // callback restores this same window without a separate status item.
        hide_ns_window(ns_window);
        false
    });

    Ok(())
}

pub(crate) fn set_keep_running_on_close(enabled: bool) {
    KEEP_RUNNING_ON_CLOSE.store(enabled, Ordering::SeqCst);
}

pub(crate) fn show_main_window() {
    let ptr = MAIN_WINDOW.load(Ordering::SeqCst) as *mut NSWindow;
    if !ptr.is_null() {
        show_ns_window(ptr);
    }
}

pub(crate) fn hide_main_window() {
    let ptr = MAIN_WINDOW.load(Ordering::SeqCst) as *mut NSWindow;
    if !ptr.is_null() {
        hide_ns_window(ptr);
    }
}

pub(crate) fn request_quit() {
    QUIT_REQUESTED.store(true, Ordering::SeqCst);
}

fn main_window(window: &Window) -> anyhow::Result<*mut NSWindow> {
    let handle = raw_window_handle::HasWindowHandle::window_handle(window)
        .map_err(|_| anyhow!("unable to read macOS window handle"))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(anyhow!("OxideTerm main window is not an AppKit window"));
    };
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    view.window()
        .map(|window| Retained::as_ptr(&window) as *mut NSWindow)
        .ok_or_else(|| anyhow!("AppKit view is not attached to an NSWindow"))
}

fn show_ns_window(window: *mut NSWindow) {
    unsafe {
        let window = &*window;
        window.deminiaturize(None);
        window.makeKeyAndOrderFront(None);
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        NSApplication::sharedApplication(mtm).activate();
    }
}

fn hide_ns_window(window: *mut NSWindow) {
    unsafe {
        (&*window).orderOut(None);
    }
}
