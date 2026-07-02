#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
pub(crate) use macos::*;
#[cfg(target_os = "windows")]
pub(crate) use windows::*;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn install_for_window(
    _window: &mut gpui::Window,
    _cx: &gpui::App,
    _menu: crate::DesktopPresenceMenu,
    _tx: std::sync::mpsc::Sender<crate::DesktopPresenceEvent>,
) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn set_keep_running_on_close(_enabled: bool) {}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn show_main_window() {}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn hide_main_window() {}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn request_quit() {}
