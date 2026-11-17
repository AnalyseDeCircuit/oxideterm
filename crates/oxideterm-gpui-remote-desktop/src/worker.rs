// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopFrameDeliverySlot, RemoteDesktopHelperEvent,
    RemoteDesktopHelperRequest, RemoteDesktopMonitorLayout, RemoteDesktopProviderManifest,
    RemoteDesktopSize,
};
use oxideterm_workspace::TabId;
use tokio::sync::Notify;

#[derive(Debug)]
pub enum RemoteDesktopWorkerDelivery {
    FrameReady {
        tab_id: TabId,
        generation: u64,
    },
    FrameRecoveryRequired {
        tab_id: TabId,
        generation: u64,
    },
    Event {
        tab_id: TabId,
        generation: u64,
        event: RemoteDesktopHelperEvent,
    },
    TransportFailed {
        tab_id: TabId,
        generation: u64,
        message: String,
    },
}

#[derive(Clone)]
pub struct RemoteDesktopWorkerWake {
    pending: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    notification: Arc<Notify>,
}

impl Default for RemoteDesktopWorkerWake {
    fn default() -> Self {
        Self {
            pending: Arc::new(AtomicBool::new(false)),
            stopped: Arc::new(AtomicBool::new(false)),
            notification: Arc::new(Notify::new()),
        }
    }
}

impl RemoteDesktopWorkerWake {
    pub fn mark(&self) {
        // Worker threads cannot touch GPUI state directly. Notify stores one
        // permit when the foreground task has not started waiting yet.
        self.pending.store(true, Ordering::Release);
        self.notification.notify_one();
    }

    pub fn take(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.notification.notify_one();
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    pub async fn wait(&self) {
        self.notification.notified().await;
    }
}

fn send_remote_desktop_worker_delivery(
    delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    worker_wake: &RemoteDesktopWorkerWake,
    delivery: RemoteDesktopWorkerDelivery,
) {
    // Publish before waking so the foreground task cannot observe an empty queue.
    if delivery_tx.send(delivery).is_ok() {
        worker_wake.mark();
    }
}

pub struct RemoteDesktopWorkerOwner {
    request_tx: Option<mpsc::Sender<RemoteDesktopHelperRequest>>,
    worker_thread: Option<thread::JoinHandle<()>>,
}

impl RemoteDesktopWorkerOwner {
    pub fn spawn(
        tab_id: TabId,
        generation: u64,
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        password_available: bool,
        frame_slot: RemoteDesktopFrameDeliverySlot,
        worker_wake: RemoteDesktopWorkerWake,
        initial_size: RemoteDesktopSize,
        scale_factor: Option<u32>,
        monitor_layout: RemoteDesktopMonitorLayout,
        delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
    ) -> Self {
        let (request_tx, request_rx) = mpsc::channel();
        let worker_thread = thread::Builder::new()
            .name(format!("remote-desktop-{}", tab_id.0))
            .spawn(move || {
                run_remote_desktop_worker(
                    tab_id,
                    generation,
                    profile,
                    provider,
                    password_available,
                    initial_size,
                    scale_factor,
                    monitor_layout,
                    frame_slot,
                    worker_wake,
                    request_rx,
                    delivery_tx,
                );
            })
            .expect("failed to start remote desktop worker");
        Self::new(request_tx, worker_thread)
    }

    pub fn new(
        request_tx: mpsc::Sender<RemoteDesktopHelperRequest>,
        worker_thread: thread::JoinHandle<()>,
    ) -> Self {
        Self {
            request_tx: Some(request_tx),
            worker_thread: Some(worker_thread),
        }
    }

    pub fn request_sender(&self) -> Option<&mpsc::Sender<RemoteDesktopHelperRequest>> {
        self.request_tx.as_ref()
    }

    pub fn request_sender_cloned(&self) -> Option<mpsc::Sender<RemoteDesktopHelperRequest>> {
        self.request_tx.clone()
    }

    pub fn send(&self, request: RemoteDesktopHelperRequest) -> bool {
        self.request_tx
            .as_ref()
            .is_some_and(|request_tx| request_tx.send(request).is_ok())
    }

    pub fn shutdown(&mut self) {
        let worker_thread = self.close_and_take_thread();
        Self::retire_worker_thread(worker_thread);
    }

    pub fn close_and_take_thread(&mut self) -> Option<thread::JoinHandle<()>> {
        if let Some(request_tx) = self.request_tx.take() {
            // Session shutdown and helper replacement release input state before
            // asking the helper to exit cooperatively.
            let _ = request_tx.send(RemoteDesktopHelperRequest::ReleaseAllInputs);
            let _ = request_tx.send(RemoteDesktopHelperRequest::Close);
        }
        self.worker_thread.take()
    }

    fn retire_worker_thread(worker_thread: Option<thread::JoinHandle<()>>) {
        let Some(worker_thread) = worker_thread else {
            return;
        };
        if worker_thread.is_finished() {
            let _ = worker_thread.join();
            return;
        }

        let reaper_name = worker_thread
            .thread()
            .name()
            .map(|name| format!("{name}-reaper"))
            .unwrap_or_else(|| "remote-desktop-worker-reaper".to_string());
        // The protocol worker has its own helper timeout, so this reaper owns a
        // bounded join without blocking the GPUI thread during session teardown.
        let _ = thread::Builder::new().name(reaper_name).spawn(move || {
            let _ = worker_thread.join();
        });
    }
}

impl Drop for RemoteDesktopWorkerOwner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_remote_desktop_worker(
    tab_id: TabId,
    generation: u64,
    profile: RemoteDesktopConnectionProfile,
    provider: RemoteDesktopProviderManifest,
    password_available: bool,
    initial_size: RemoteDesktopSize,
    scale_factor: Option<u32>,
    monitor_layout: RemoteDesktopMonitorLayout,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    worker_wake: RemoteDesktopWorkerWake,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    let worker_id = oxideterm_remote_desktop::RemoteDesktopWorkerId::new(
        oxideterm_remote_desktop::RemoteDesktopSessionId::new(),
        generation,
    );
    let (domain_delivery_tx, domain_delivery_rx) = mpsc::channel();
    let bridge_wake = worker_wake.clone();
    let bridge_thread = thread::Builder::new()
        .name(format!("remote-desktop-delivery-{}", tab_id.0))
        .spawn(move || {
            while let Ok(delivery) = domain_delivery_rx.recv() {
                let delivery = map_remote_desktop_worker_delivery(tab_id, generation, delivery);
                send_remote_desktop_worker_delivery(&delivery_tx, &bridge_wake, delivery);
            }
        })
        .ok();

    oxideterm_remote_desktop::run_remote_desktop_worker(
        oxideterm_remote_desktop::RemoteDesktopWorkerConfig {
            worker_id,
            profile,
            provider,
            password_available,
            initial_size,
            scale_factor,
            monitor_layout,
        },
        frame_slot,
        request_rx,
        domain_delivery_tx,
    );
    if let Some(bridge_thread) = bridge_thread {
        // Stop only after all domain deliveries have crossed the TabId adapter.
        let _ = bridge_thread.join();
    }
    worker_wake.stop();
}

fn map_remote_desktop_worker_delivery(
    tab_id: TabId,
    generation: u64,
    delivery: oxideterm_remote_desktop::RemoteDesktopWorkerDelivery,
) -> RemoteDesktopWorkerDelivery {
    match delivery {
        oxideterm_remote_desktop::RemoteDesktopWorkerDelivery::FrameReady { worker_id } => {
            debug_assert_eq!(worker_id.request_id, generation);
            RemoteDesktopWorkerDelivery::FrameReady { tab_id, generation }
        }
        oxideterm_remote_desktop::RemoteDesktopWorkerDelivery::FrameRecoveryRequired {
            worker_id,
        } => {
            debug_assert_eq!(worker_id.request_id, generation);
            RemoteDesktopWorkerDelivery::FrameRecoveryRequired { tab_id, generation }
        }
        oxideterm_remote_desktop::RemoteDesktopWorkerDelivery::Event { worker_id, event } => {
            debug_assert_eq!(worker_id.request_id, generation);
            RemoteDesktopWorkerDelivery::Event {
                tab_id,
                generation,
                event,
            }
        }
        oxideterm_remote_desktop::RemoteDesktopWorkerDelivery::TransportFailed {
            worker_id,
            message,
        } => {
            debug_assert_eq!(worker_id.request_id, generation);
            RemoteDesktopWorkerDelivery::TransportFailed {
                tab_id,
                generation,
                message,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_wake_uses_event_notification_and_stops_explicitly() {
        let wake = RemoteDesktopWorkerWake::default();
        let runtime = tokio::runtime::Runtime::new().unwrap();

        wake.mark();
        runtime.block_on(wake.wait());
        assert!(wake.take());

        wake.stop();
        runtime.block_on(wake.wait());
        assert!(wake.is_stopped());
    }

    #[test]
    fn worker_delivery_enqueues_before_marking_the_wake() {
        let (sender, receiver) = mpsc::channel();
        let wake = RemoteDesktopWorkerWake::default();
        let tab_id = TabId(7);

        send_remote_desktop_worker_delivery(
            &sender,
            &wake,
            RemoteDesktopWorkerDelivery::FrameReady {
                tab_id,
                generation: 3,
            },
        );

        assert!(wake.take());
        assert!(matches!(
            receiver.try_recv(),
            Ok(RemoteDesktopWorkerDelivery::FrameReady {
                tab_id: received_tab_id,
                generation: 3,
            }) if received_tab_id == tab_id
        ));
    }

    #[test]
    fn worker_deliveries_remain_isolated_by_session_mailbox() {
        let (first_sender, first_receiver) = mpsc::channel();
        let (second_sender, second_receiver) = mpsc::channel();
        let first_wake = RemoteDesktopWorkerWake::default();
        let second_wake = RemoteDesktopWorkerWake::default();

        send_remote_desktop_worker_delivery(
            &first_sender,
            &first_wake,
            RemoteDesktopWorkerDelivery::FrameReady {
                tab_id: TabId(1),
                generation: 4,
            },
        );

        assert!(first_wake.take());
        assert!(!second_wake.take());
        assert!(first_receiver.try_recv().is_ok());
        assert!(second_receiver.try_recv().is_err());

        // A second tab owns a distinct sender, receiver and wake permit.
        send_remote_desktop_worker_delivery(
            &second_sender,
            &second_wake,
            RemoteDesktopWorkerDelivery::FrameReady {
                tab_id: TabId(2),
                generation: 7,
            },
        );
        assert!(second_wake.take());
        assert!(second_receiver.try_recv().is_ok());
        assert!(first_receiver.try_recv().is_err());
    }
}
