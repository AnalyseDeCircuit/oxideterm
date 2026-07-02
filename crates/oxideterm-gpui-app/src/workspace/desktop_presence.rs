use super::*;

impl WorkspaceApp {
    pub(crate) fn start_desktop_presence_polling(&mut self, cx: &mut Context<Self>) {
        if self.desktop_presence_rx.is_none() || self.desktop_presence_polling {
            return;
        }
        self.desktop_presence_polling = true;
        cx.spawn(async move |weak, cx| {
            // Native tray/menu callbacks arrive outside GPUI's action system,
            // so a small UI-task poll bridges them back onto the workspace.
            loop {
                Timer::after(Duration::from_millis(100)).await;
                let keep_polling = weak
                    .update(cx, |this, cx| {
                        this.poll_desktop_presence_events(cx);
                        this.desktop_presence_polling
                    })
                    .unwrap_or(false);
                if !keep_polling {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_desktop_presence_events(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.desktop_presence_rx.as_ref() else {
            self.desktop_presence_polling = false;
            return;
        };

        let mut events = Vec::new();
        let mut disconnected = false;
        // Drain the channel before handling actions so callbacks cannot borrow
        // the receiver while workspace mutations are being dispatched.
        loop {
            match rx.try_recv() {
                Ok(event) => events.push(event),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        for event in events {
            self.handle_desktop_presence_event(event, cx);
        }
        if disconnected {
            self.desktop_presence_rx = None;
            self.desktop_presence_polling = false;
        }
    }

    fn handle_desktop_presence_event(
        &mut self,
        event: oxideterm_desktop_presence::DesktopPresenceEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            oxideterm_desktop_presence::DesktopPresenceEvent::ShowMainWindow => {
                oxideterm_desktop_presence::show_main_window();
            }
            oxideterm_desktop_presence::DesktopPresenceEvent::HideMainWindow => {
                oxideterm_desktop_presence::hide_main_window();
            }
            oxideterm_desktop_presence::DesktopPresenceEvent::NewConnection => {
                oxideterm_desktop_presence::show_main_window();
                cx.dispatch_action(&crate::NewConnection);
            }
            oxideterm_desktop_presence::DesktopPresenceEvent::OpenSettings => {
                oxideterm_desktop_presence::show_main_window();
                cx.dispatch_action(&crate::OpenSettings);
            }
            oxideterm_desktop_presence::DesktopPresenceEvent::CheckForUpdates => {
                oxideterm_desktop_presence::show_main_window();
                cx.dispatch_action(&crate::OpenSettings);
                self.check_native_update(cx);
            }
            oxideterm_desktop_presence::DesktopPresenceEvent::Quit => {
                oxideterm_desktop_presence::request_quit();
                cx.quit();
            }
        }
    }
}
