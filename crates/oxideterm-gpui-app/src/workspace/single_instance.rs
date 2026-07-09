// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(crate) fn start_single_instance_polling(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.single_instance_rx.is_none() || self.single_instance_polling {
            return;
        }
        self.single_instance_polling = true;
        let window_handle = window.window_handle();
        cx.spawn(async move |weak, cx| {
            // The single-instance listener runs on a standard thread. Polling
            // keeps the request handling on GPUI's window context.
            loop {
                Timer::after(Duration::from_millis(100)).await;
                let keep_polling = cx
                    .update_window(window_handle, |_, window, cx| {
                        weak.update(cx, |this, cx| {
                            this.poll_single_instance_events(window, cx);
                            this.single_instance_polling
                        })
                        .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !keep_polling {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_single_instance_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rx) = self.single_instance_rx.as_ref() else {
            self.single_instance_polling = false;
            return;
        };

        let mut events = Vec::new();
        let mut disconnected = false;
        {
            // The application owns the receiver across workspace lifetimes;
            // each active window only locks it while draining queued events.
            let rx = rx.lock().expect("single-instance receiver poisoned");
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
        }

        for event in events {
            self.handle_single_instance_event(event, window, cx);
        }
        if disconnected {
            self.single_instance_rx = None;
            self.single_instance_polling = false;
        }
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
