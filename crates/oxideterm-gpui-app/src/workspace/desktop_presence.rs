use super::*;

impl WorkspaceApp {
    pub(crate) fn start_desktop_presence_delivery(&mut self, cx: &mut Context<Self>) {
        let Some(notification) = self
            .desktop_presence_rx
            .as_ref()
            .map(|receiver| receiver.notification())
        else {
            return;
        };
        if self.poll_desktop_presence_events(cx) {
            notification.notify_one();
        }
        let stopped = Arc::new(AtomicBool::new(false));
        let stop_notification = Arc::new(tokio::sync::Notify::new());
        let release_stopped = stopped.clone();
        let release_notification = stop_notification.clone();
        cx.on_release(move |_, _| {
            release_stopped.store(true, Ordering::Release);
            release_notification.notify_one();
        })
        .detach();
        cx.spawn(async move |weak, cx| {
            loop {
                tokio::select! {
                    _ = notification.notified() => {}
                    _ = stop_notification.notified() => break,
                }
                if stopped.load(Ordering::Acquire) {
                    break;
                }
                let Ok(backlog_remaining) =
                    weak.update(cx, |this, cx| this.poll_desktop_presence_events(cx))
                else {
                    break;
                };
                if backlog_remaining {
                    notification.notify_one();
                }
            }
        })
        .detach();
    }

    fn poll_desktop_presence_events(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(rx) = self.desktop_presence_rx.as_ref() else {
            return false;
        };

        let started_at = Instant::now();
        let mut events = Vec::new();
        let mut source_exhausted = false;
        let mut disconnected = false;
        // Drain the channel before handling actions so callbacks cannot borrow
        // the receiver while workspace mutations are being dispatched.
        while delivery::USER_ACTION_DELIVERY_BUDGET.allows_next(events.len(), started_at.elapsed())
        {
            match rx.try_recv() {
                Ok(event) => events.push(event),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    source_exhausted = true;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    source_exhausted = true;
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
        }
        !source_exhausted
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
