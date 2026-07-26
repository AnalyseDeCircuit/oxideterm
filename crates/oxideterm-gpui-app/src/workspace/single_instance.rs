// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(crate) fn start_single_instance_delivery(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(notification) = self
            .single_instance_rx
            .as_ref()
            .map(|receiver| receiver.notification())
        else {
            return;
        };
        if self.poll_single_instance_events(window, cx) {
            notification.notify_one();
        }
        let window_handle = window.window_handle();
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
                let backlog_remaining = cx
                    .update_window(window_handle, |_, window, cx| {
                        weak.update(cx, |this, cx| this.poll_single_instance_events(window, cx))
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if backlog_remaining {
                    notification.notify_one();
                }
            }
        })
        .detach();
    }

    fn poll_single_instance_events(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(rx) = self.single_instance_rx.as_ref() else {
            return false;
        };

        let started_at = Instant::now();
        let mut events = Vec::new();
        let mut source_exhausted = false;
        let mut disconnected = false;
        {
            // The application owns the receiver across workspace lifetimes;
            // each active window only locks it while draining queued events.
            let rx = rx.lock().expect("single-instance receiver poisoned");
            while delivery::USER_ACTION_DELIVERY_BUDGET
                .allows_next(events.len(), started_at.elapsed())
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
        }

        for event in events {
            self.handle_single_instance_event(event, window, cx);
        }
        if disconnected {
            self.single_instance_rx = None;
        }
        !source_exhausted
    }

    fn handle_single_instance_event(
        &mut self,
        event: crate::single_instance::SingleInstanceEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            crate::single_instance::SingleInstanceEvent::ShowMainWindow => {
                oxideterm_desktop_presence::show_main_window();
            }
            crate::single_instance::SingleInstanceEvent::OpenTemporarySsh(launch) => {
                oxideterm_desktop_presence::show_main_window();
                if let Err(error) = self.open_temporary_ssh_launch(launch, window, cx) {
                    eprintln!("failed to open forwarded SSH launch: {error:#}");
                }
            }
        }
    }
}
