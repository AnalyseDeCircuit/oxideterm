//! Native desktop presence integration for OxideTerm.
//!
//! This crate owns platform status-entry behavior: Windows notification-area
//! icons and macOS menu-bar status items. The GPUI app remains responsible for
//! window routing, settings persistence, and business actions.

mod config;
mod event;
mod platform;

use std::sync::mpsc;

use gpui::{App, Window};

pub use config::DesktopPresenceMenu;
pub use event::DesktopPresenceEvent;

pub type DesktopPresenceReceiver = mpsc::Receiver<DesktopPresenceEvent>;

pub fn install_for_window(
    window: &mut Window,
    cx: &App,
    menu: DesktopPresenceMenu,
) -> anyhow::Result<Option<DesktopPresenceReceiver>> {
    let (tx, rx) = mpsc::channel();
    platform::install_for_window(window, cx, menu, tx)?;
    // Only the Windows tray currently emits application events. macOS keeps
    // its close-to-background behavior without registering a status item.
    Ok(cfg!(target_os = "windows").then_some(rx))
}

pub fn set_keep_running_on_close(enabled: bool) {
    platform::set_keep_running_on_close(enabled);
}

pub fn show_main_window() {
    platform::show_main_window();
}

pub fn hide_main_window() {
    platform::hide_main_window();
}

pub fn request_quit() {
    platform::request_quit();
}
