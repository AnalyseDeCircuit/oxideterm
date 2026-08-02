//! Native desktop presence integration for OxideTerm.
//!
//! This crate owns platform status-entry behavior: Windows notification-area
//! icons and macOS menu-bar status items. The GPUI app remains responsible for
//! window routing, settings persistence, and business actions.

mod config;
mod event;
mod platform;

use std::sync::{Arc, mpsc};
use tokio::sync::Notify;

#[cfg(target_os = "windows")]
use std::path::Path;

use gpui::{App, Window};

pub use config::DesktopPresenceMenu;
pub use event::DesktopPresenceEvent;

pub struct DesktopPresenceReceiver {
    receiver: mpsc::Receiver<DesktopPresenceEvent>,
    notification: Arc<Notify>,
}

impl DesktopPresenceReceiver {
    pub fn try_recv(&self) -> Result<DesktopPresenceEvent, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }

    pub fn notification(&self) -> Arc<Notify> {
        self.notification.clone()
    }
}

#[derive(Clone)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) struct DesktopPresenceDeliverySender {
    sender: mpsc::Sender<DesktopPresenceEvent>,
    notification: Arc<Notify>,
}

impl DesktopPresenceDeliverySender {
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    fn send(
        &self,
        event: DesktopPresenceEvent,
    ) -> Result<(), mpsc::SendError<DesktopPresenceEvent>> {
        self.sender.send(event)?;
        self.notification.notify_one();
        Ok(())
    }
}

fn desktop_presence_channel() -> (DesktopPresenceDeliverySender, DesktopPresenceReceiver) {
    let (sender, receiver) = mpsc::channel();
    let notification = Arc::new(Notify::new());
    (
        DesktopPresenceDeliverySender {
            sender,
            notification: notification.clone(),
        },
        DesktopPresenceReceiver {
            receiver,
            notification,
        },
    )
}

pub fn install_for_window(
    window: &mut Window,
    cx: &App,
    menu: DesktopPresenceMenu,
) -> anyhow::Result<Option<DesktopPresenceReceiver>> {
    let (tx, rx) = desktop_presence_channel();
    platform::install_for_window(window, cx, menu, tx)?;
    // Only the Windows tray currently emits application events. macOS keeps
    // its close-to-background behavior without registering a status item.
    Ok(cfg!(target_os = "windows").then_some(rx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn delivery_notifies_after_event_is_queued() {
        let (sender, receiver) = desktop_presence_channel();
        let notification = receiver.notification();

        sender.send(DesktopPresenceEvent::ShowMainWindow).unwrap();
        notification.notified().await;

        assert!(matches!(
            receiver.try_recv().unwrap(),
            DesktopPresenceEvent::ShowMainWindow
        ));
    }
}

pub fn set_keep_running_on_close(enabled: bool) {
    platform::set_keep_running_on_close(enabled);
}

pub fn install_main_window_close_guard(
    window: &mut Window,
    cx: &App,
    guard: impl Fn(&mut Window, &mut App) -> bool + 'static,
) {
    platform::install_main_window_close_guard(window, cx, guard);
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

#[cfg(target_os = "windows")]
pub fn set_application_icon(icon_path: &Path) -> anyhow::Result<()> {
    platform::set_application_icon(icon_path)
}
