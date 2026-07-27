// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use std::sync::{Arc, Mutex};

/// Moves a bounded notice batch into the window toast adapter exactly once.
#[derive(Clone)]
pub(in crate::workspace) struct TerminalNoticeBatchRequest {
    notices: Arc<Mutex<Option<Vec<TerminalNotice>>>>,
}

impl TerminalNoticeBatchRequest {
    fn new(notices: Vec<TerminalNotice>) -> Self {
        Self {
            notices: Arc::new(Mutex::new(Some(notices))),
        }
    }

    pub(in crate::workspace) fn take(&self) -> Option<Vec<TerminalNotice>> {
        self.notices.lock().ok()?.take()
    }
}

#[derive(Clone)]
pub(in crate::workspace) enum WorkspaceTerminalEvent {
    NoticesReady(TerminalNoticeBatchRequest),
}

/// Owns terminal-wide delivery channels and their foreground cancellation lifecycle.
pub(in crate::workspace) struct WorkspaceTerminalEntity {
    notice_tx: delivery::ActiveDeliverySender<TerminalNotice>,
    notice_rx: std::sync::mpsc::Receiver<TerminalNotice>,
}

impl WorkspaceTerminalEntity {
    pub(in crate::workspace) fn new(cx: &mut Context<Self>) -> Self {
        let (notice_tx, notice_rx) = delivery::ActiveDeliverySender::channel();
        let notice_wake = notice_tx.wake();
        let release_wake = notice_wake.clone();
        cx.on_release(move |_, _| {
            // External sinks may outlive the UI owner, so release must stop
            // the foreground waiter independently of sender lifetime.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |terminal, cx| {
            loop {
                notice_wake.wait().await;
                let should_drain = notice_wake.take();
                let stopped = notice_wake.is_stopped();
                if !should_drain {
                    if stopped {
                        break;
                    }
                    continue;
                }
                let backlog_remaining = terminal
                    .update(cx, |terminal, cx| terminal.drain_notices(cx))
                    .unwrap_or(false);
                if backlog_remaining {
                    // Preserve bounded batches while guaranteeing eventual delivery.
                    notice_wake.mark();
                } else if stopped {
                    break;
                }
            }
        })
        .detach();

        Self {
            notice_tx,
            notice_rx,
        }
    }

    pub(in crate::workspace) fn notice_sender(
        &self,
    ) -> delivery::ActiveDeliverySender<TerminalNotice> {
        // The root keeps one producer capability for legacy surface adapters;
        // receiver state and foreground delivery remain Entity-owned.
        self.notice_tx.clone()
    }

    fn drain_notices(&mut self, cx: &mut Context<Self>) -> bool {
        let delivery_batch =
            delivery::drain_channel(&self.notice_rx, delivery::NOTIFICATION_DELIVERY_BUDGET);
        if !delivery_batch.items.is_empty() {
            cx.emit(WorkspaceTerminalEvent::NoticesReady(
                TerminalNoticeBatchRequest::new(delivery_batch.items),
            ));
        }
        delivery_batch.outcome.backlog_remaining
    }
}

impl gpui::EventEmitter<WorkspaceTerminalEvent> for WorkspaceTerminalEntity {}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    struct TerminalEventRecorder {
        notices: Vec<TerminalNoticeBatchRequest>,
        _subscription: Subscription,
    }

    #[gpui::test]
    fn notice_delivery_is_entity_owned_and_payload_is_consumed_once(cx: &mut TestAppContext) {
        let terminal = cx.new(WorkspaceTerminalEntity::new);
        let recorder = cx.new(|cx| {
            let subscription = cx.subscribe(
                &terminal,
                |recorder: &mut TerminalEventRecorder, _terminal, event, _cx| match event {
                    WorkspaceTerminalEvent::NoticesReady(request) => {
                        recorder.notices.push(request.clone());
                    }
                },
            );
            TerminalEventRecorder {
                notices: Vec::new(),
                _subscription: subscription,
            }
        });
        let sender = terminal.read_with(cx, |terminal, _cx| terminal.notice_sender());

        sender
            .send(TerminalNotice {
                title: "ready".to_string(),
                description: Some("description".to_string()),
                status_text: None,
                progress: None,
                variant: TerminalNoticeVariant::Success,
            })
            .expect("notice send");
        cx.run_until_parked();

        let request = recorder.read_with(cx, |recorder, _cx| recorder.notices[0].clone());
        let notices = request.take().expect("notice payload");
        let notice = &notices[0];
        assert_eq!(notice.title, "ready");
        assert_eq!(notice.description.as_deref(), Some("description"));
        assert!(request.take().is_none());
    }

    #[gpui::test]
    fn entity_release_stops_notice_delivery_waiter(cx: &mut TestAppContext) {
        let terminal = cx.new(WorkspaceTerminalEntity::new);
        let notice_wake = terminal.read_with(cx, |terminal, _cx| terminal.notice_sender().wake());

        drop(terminal);
        cx.update(|_cx| {});
        cx.run_until_parked();

        assert!(notice_wake.is_stopped());
    }
}
