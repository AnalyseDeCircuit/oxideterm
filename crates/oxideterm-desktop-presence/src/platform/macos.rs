use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
};

use anyhow::anyhow;
use gpui::{App, Window};
use objc2::{
    AnyThread, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained, runtime::AnyObject, sel,
};
use objc2_app_kit::{
    NSApplication, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength, NSView, NSWindow,
};
use objc2_foundation::{NSData, NSObject, NSObjectProtocol, NSSize, NSString};
use raw_window_handle::RawWindowHandle;

use crate::{DesktopPresenceEvent, DesktopPresenceMenu};

static MAIN_WINDOW: AtomicUsize = AtomicUsize::new(0);
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static KEEP_RUNNING_ON_CLOSE: AtomicBool = AtomicBool::new(true);

thread_local! {
    static STATUS_ITEM: RefCell<Option<MacStatusItem>> = const { RefCell::new(None) };
}

struct MacStatusItem {
    _item: Retained<NSStatusItem>,
    _menu: Retained<NSMenu>,
    _target: Retained<MenuTarget>,
}

#[derive(Clone)]
struct MenuTargetIvars {
    tx: mpsc::Sender<DesktopPresenceEvent>,
}

define_class!(
    // SAFETY:
    // - NSObject has no extra subclassing requirements for action targets.
    // - MenuTarget only receives AppKit menu actions on the main thread.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = MenuTargetIvars]
    #[name = "OxideTermDesktopPresenceMenuTarget"]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(showMainWindow:))]
        fn show_main_window(&self, _sender: &AnyObject) {
            let _ = self.ivars().tx.send(DesktopPresenceEvent::ShowMainWindow);
        }

        #[unsafe(method(hideMainWindow:))]
        fn hide_main_window(&self, _sender: &AnyObject) {
            let _ = self.ivars().tx.send(DesktopPresenceEvent::HideMainWindow);
        }

        #[unsafe(method(newConnection:))]
        fn new_connection(&self, _sender: &AnyObject) {
            let _ = self.ivars().tx.send(DesktopPresenceEvent::NewConnection);
        }

        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: &AnyObject) {
            let _ = self.ivars().tx.send(DesktopPresenceEvent::OpenSettings);
        }

        #[unsafe(method(checkForUpdates:))]
        fn check_for_updates(&self, _sender: &AnyObject) {
            let _ = self.ivars().tx.send(DesktopPresenceEvent::CheckForUpdates);
        }

        #[unsafe(method(quitOxideTerm:))]
        fn quit_oxideterm(&self, _sender: &AnyObject) {
            let _ = self.ivars().tx.send(DesktopPresenceEvent::Quit);
        }
    }

    unsafe impl NSObjectProtocol for MenuTarget {}
);

impl MenuTarget {
    fn new(tx: mpsc::Sender<DesktopPresenceEvent>, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(MenuTargetIvars { tx });
        unsafe { msg_send![super(this), init] }
    }
}

pub(crate) fn install_for_window(
    window: &mut Window,
    cx: &App,
    menu: DesktopPresenceMenu,
    tx: mpsc::Sender<DesktopPresenceEvent>,
) -> anyhow::Result<()> {
    let ns_window = main_window(window)?;
    MAIN_WINDOW.store(ns_window as usize, Ordering::SeqCst);
    install_status_item(menu, tx)?;

    window.on_window_should_close(cx, move |_window, _cx| {
        if QUIT_REQUESTED.load(Ordering::SeqCst) || !KEEP_RUNNING_ON_CLOSE.load(Ordering::SeqCst) {
            return true;
        }

        // macOS menu-bar residency should not close the GPUI window tree; it
        // only orders the native window out until the status item restores it.
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
    STATUS_ITEM.with(|slot| {
        if let Some(item) = slot.borrow_mut().take() {
            let status_bar = NSStatusBar::systemStatusBar();
            status_bar.removeStatusItem(&item._item);
        }
    });
}

fn install_status_item(
    menu: DesktopPresenceMenu,
    tx: mpsc::Sender<DesktopPresenceEvent>,
) -> anyhow::Result<()> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Err(anyhow!(
            "macOS status item must be installed on the main thread"
        ));
    };

    let status_bar = NSStatusBar::systemStatusBar();
    let item = status_bar.statusItemWithLength(NSVariableStatusItemLength);
    let target = MenuTarget::new(tx, mtm);
    if let Some(button) = item.button(mtm) {
        if let Some(icon) = menu.status_icon {
            if let Some(image) = template_image_from_png(icon) {
                button.setImage(Some(&image));
                button.setTitle(&NSString::from_str(""));
            } else {
                button.setTitle(&NSString::from_str(&menu.status_title));
            }
        } else {
            button.setTitle(&NSString::from_str(&menu.status_title));
        }
        button.setToolTip(Some(&NSString::from_str(&menu.app_name)));
    }

    let native_menu =
        NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(&menu.app_name));
    add_action_item(
        &native_menu,
        &target,
        &menu.show_main_window,
        sel!(showMainWindow:),
        mtm,
    );
    add_action_item(
        &native_menu,
        &target,
        &menu.hide_main_window,
        sel!(hideMainWindow:),
        mtm,
    );
    native_menu.addItem(&NSMenuItem::separatorItem(mtm));
    add_action_item(
        &native_menu,
        &target,
        &menu.new_connection,
        sel!(newConnection:),
        mtm,
    );
    add_action_item(
        &native_menu,
        &target,
        &menu.settings,
        sel!(openSettings:),
        mtm,
    );
    add_action_item(
        &native_menu,
        &target,
        &menu.check_for_updates,
        sel!(checkForUpdates:),
        mtm,
    );
    native_menu.addItem(&NSMenuItem::separatorItem(mtm));
    add_action_item(&native_menu, &target, &menu.quit, sel!(quitOxideTerm:), mtm);
    item.setMenu(Some(&native_menu));

    STATUS_ITEM.with(|slot| {
        *slot.borrow_mut() = Some(MacStatusItem {
            _item: item,
            _menu: native_menu,
            _target: target,
        });
    });
    Ok(())
}

fn template_image_from_png(icon: crate::DesktopPresenceIcon) -> Option<Retained<NSImage>> {
    // Template images use alpha as the glyph shape; AppKit supplies the
    // foreground color so the status item matches light and dark menu bars.
    let data = unsafe {
        NSData::dataWithBytes_length(
            icon.template_png_bytes.as_ptr().cast(),
            icon.template_png_bytes.len(),
        )
    };
    let image = NSImage::initWithData(NSImage::alloc(), &data)?;
    image.setTemplate(true);
    image.setSize(NSSize::new(icon.point_size, icon.point_size));
    Some(image)
}

fn add_action_item(
    menu: &NSMenu,
    target: &MenuTarget,
    title: &str,
    action: objc2::runtime::Sel,
    mtm: MainThreadMarker,
) {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            &NSString::from_str(""),
        )
    };
    unsafe {
        item.setTarget(Some(target.as_super().as_super()));
    }
    menu.addItem(&item);
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
