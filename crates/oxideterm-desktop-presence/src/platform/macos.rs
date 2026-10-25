use std::{
    cell::RefCell,
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::anyhow;
use gpui::{App, Subscription, Window};
use objc2::{MainThreadMarker, rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{
    NSApplication, NSView, NSWindow, NSWindowDidExitFullScreenNotification, NSWindowStyleMask,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol};
use raw_window_handle::RawWindowHandle;

use crate::{DesktopPresenceDeliverySender, DesktopPresenceMenu};

static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static KEEP_RUNNING_ON_CLOSE: AtomicBool = AtomicBool::new(true);

thread_local! {
    static MAIN_WINDOW: RefCell<Option<MainWindowPresence>> = const { RefCell::new(None) };
}

/// Keeps native callbacks and deferred hiding scoped to the registered main window.
struct MainWindowPresence {
    address: usize,
    hide_pending: Arc<AtomicBool>,
    exiting_fullscreen: Arc<AtomicBool>,
    observer: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    _closed_subscription: Subscription,
}

impl MainWindowPresence {
    fn new(ns_window: *mut NSWindow, window: &Window, cx: &App) -> Self {
        let address = ns_window as usize;
        let hide_pending = Arc::new(AtomicBool::new(false));
        let exiting_fullscreen = Arc::new(AtomicBool::new(false));
        let completion_hide_pending = hide_pending.clone();
        let completion_exiting_fullscreen = exiting_fullscreen.clone();
        let completion = block2::RcBlock::new(move |_: NonNull<NSNotification>| {
            completion_exiting_fullscreen.store(false, Ordering::SeqCst);
            if completion_hide_pending.swap(false, Ordering::SeqCst) {
                // AppKit posts this window-scoped notification synchronously on the main thread.
                let _mtm = MainThreadMarker::new()
                    .expect("fullscreen completion must run on the main thread");
                unsafe {
                    (&*(address as *mut NSWindow)).orderOut(None);
                }
            }
        });
        let observer = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSWindowDidExitFullScreenNotification),
                Some(&*ns_window),
                None,
                &completion,
            )
        };
        let window_id = window.window_handle().window_id();
        let closed_subscription = cx.on_window_closed(move |_, closed_id| {
            if closed_id == window_id {
                clear_window_tracking(address);
            }
        });
        Self {
            address,
            hide_pending,
            exiting_fullscreen,
            observer,
            _closed_subscription: closed_subscription,
        }
    }
}

impl Drop for MainWindowPresence {
    fn drop(&mut self) {
        self.hide_pending.store(false, Ordering::SeqCst);
        unsafe {
            NSNotificationCenter::defaultCenter().removeObserver((*self.observer).as_ref());
        }
    }
}

fn clear_window_tracking(address: usize) {
    let removed = MAIN_WINDOW.with(|window| {
        let mut window = window.borrow_mut();
        if window
            .as_ref()
            .is_some_and(|window| window.address == address)
        {
            window.take()
        } else {
            None
        }
    });
    // Unregister native callbacks outside the owner borrow in case AppKit re-enters.
    drop(removed);
}

pub(crate) fn install_for_window(
    window: &mut Window,
    cx: &App,
    _menu: DesktopPresenceMenu,
    _tx: DesktopPresenceDeliverySender,
) -> anyhow::Result<()> {
    let presence = MainWindowPresence::new(main_window(window)?, window, cx);
    let previous = MAIN_WINDOW.with(|window| window.replace(Some(presence)));
    drop(previous);
    install_main_window_close_guard(window, cx, |_, _| true);
    Ok(())
}

pub(crate) fn set_keep_running_on_close(enabled: bool) {
    KEEP_RUNNING_ON_CLOSE.store(enabled, Ordering::SeqCst);
}

pub(crate) fn install_main_window_close_guard(
    window: &mut Window,
    cx: &App,
    guard: impl Fn(&mut Window, &mut App) -> bool + 'static,
) {
    let ns_window = main_window(window).ok();
    window.on_window_should_close(cx, move |window, cx| {
        if QUIT_REQUESTED.load(Ordering::SeqCst) {
            if let Some(ns_window) = ns_window {
                clear_window_tracking(ns_window as usize);
            }
            return true;
        }
        if KEEP_RUNNING_ON_CLOSE.load(Ordering::SeqCst) {
            if let Some(ns_window) = ns_window {
                hide_ns_window(ns_window);
            }
            return false;
        }
        if !guard(window, cx) {
            return false;
        }
        if let Some(ns_window) = ns_window {
            // Clear the non-owning pointer before GPUI releases the native window.
            clear_window_tracking(ns_window as usize);
        }
        true
    });
}

pub(crate) fn show_main_window() {
    let address = MAIN_WINDOW.with(|window| {
        window.borrow().as_ref().map(|window| {
            // Restoring during the transition must cancel its delayed hide.
            window.hide_pending.store(false, Ordering::SeqCst);
            window.address
        })
    });
    if let Some(address) = address {
        show_ns_window(address as *mut NSWindow);
    }
}

pub(crate) fn hide_main_window() {
    let address = MAIN_WINDOW.with(|window| window.borrow().as_ref().map(|window| window.address));
    if let Some(address) = address {
        hide_ns_window(address as *mut NSWindow);
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
    let (hide_pending, exiting_fullscreen) = MAIN_WINDOW.with(|presence| {
        let presence = presence.borrow();
        let presence = presence
            .as_ref()
            .filter(|presence| presence.address == window as usize)
            .expect("main window presence must own its fullscreen transition");
        (
            presence.hide_pending.clone(),
            presence.exiting_fullscreen.clone(),
        )
    });
    unsafe {
        let window = &*window;
        if window.styleMask().contains(NSWindowStyleMask::FullScreen)
            || exiting_fullscreen.load(Ordering::SeqCst)
        {
            hide_pending.store(true, Ordering::SeqCst);
            if !exiting_fullscreen.swap(true, Ordering::SeqCst) {
                // Order out only after AppKit removes the fullscreen Space.
                window.toggleFullScreen(None);
            }
        } else {
            window.orderOut(None);
        }
    }
}
