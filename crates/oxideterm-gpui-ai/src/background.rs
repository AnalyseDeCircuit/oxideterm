// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

mod tools;
pub use tools::AiBackgroundNotice;
#[cfg(not(target_family = "wasm"))]
pub use tools::{AiBackgroundRead, AiBackgroundReadPlan};

#[cfg(not(target_family = "wasm"))]
mod native;
#[cfg(not(target_family = "wasm"))]
pub use native::AiBackgroundExecutionRequest;

use gpui::{Context, EventEmitter};
use oxideterm_ai_tasks::{
    BackgroundTaskEvent, BackgroundTaskId, BackgroundTaskSnapshot, BackgroundTaskSpec,
    BackgroundTaskValidationError,
};
use oxideterm_workspace::delivery;

pub trait AiBackgroundHost {
    fn create(
        &self,
        spec: BackgroundTaskSpec,
    ) -> Result<BackgroundTaskId, BackgroundTaskValidationError>;
    fn snapshots_for_owner(&self, conversation_id: &str) -> Vec<BackgroundTaskSnapshot>;
    fn snapshot(&self, id: &BackgroundTaskId) -> Option<BackgroundTaskSnapshot>;
    fn cancel(&self, id: &BackgroundTaskId) -> bool;
    fn cancel_owner(&self, conversation_id: &str) -> usize;
    fn cancel_all(&self) -> usize;
    /// Stop this workspace's event producer and release its recurring jobs.
    fn shutdown(&self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiBackgroundTaskEvent {
    DeliveryReady,
}

pub struct AiBackgroundTaskEntity {
    runtime: Box<dyn AiBackgroundHost>,
    delivery_wake: delivery::ActiveDeliveryWake,
    #[cfg(not(target_family = "wasm"))]
    request_rx: Option<std::sync::mpsc::Receiver<AiBackgroundExecutionRequest>>,
    event_rx: std::sync::mpsc::Receiver<BackgroundTaskEvent>,
}

impl AiBackgroundTaskEntity {
    pub fn with_bridge(
        start: impl FnOnce(
            delivery::ActiveDeliverySender<BackgroundTaskEvent>,
        ) -> Box<dyn AiBackgroundHost>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (events, event_rx) = delivery::ActiveDeliverySender::channel();
        let delivery_wake = events.wake();
        let entity = Self {
            runtime: start(events),
            delivery_wake,
            event_rx,
            #[cfg(not(target_family = "wasm"))]
            request_rx: None,
        };
        entity.schedule_delivery(cx);
        entity
    }

    pub fn create(
        &self,
        spec: BackgroundTaskSpec,
    ) -> Result<BackgroundTaskId, BackgroundTaskValidationError> {
        self.runtime.create(spec)
    }

    pub fn snapshots_for_owner(&self, conversation_id: &str) -> Vec<BackgroundTaskSnapshot> {
        self.runtime.snapshots_for_owner(conversation_id)
    }

    pub fn snapshot_for_owner(
        &self,
        conversation_id: &str,
        task_id: &BackgroundTaskId,
    ) -> Option<BackgroundTaskSnapshot> {
        self.runtime
            .snapshot(task_id)
            .filter(|snapshot| snapshot.owner.conversation_id == conversation_id)
    }

    pub fn cancel_for_owner(&self, conversation_id: &str, task_id: &BackgroundTaskId) -> bool {
        self.snapshot_for_owner(conversation_id, task_id)
            .is_some_and(|_| self.runtime.cancel(task_id))
    }

    pub fn cancel_owner(&self, conversation_id: &str) -> usize {
        self.runtime.cancel_owner(conversation_id)
    }

    pub fn cancel_all(&self) -> usize {
        self.runtime.cancel_all()
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn take_execution_requests(&self) -> Vec<AiBackgroundExecutionRequest> {
        self.request_rx
            .as_ref()
            .map(|rx| delivery::drain_channel(rx, delivery::USER_ACTION_DELIVERY_BUDGET).items)
            .unwrap_or_default()
    }

    pub fn take_events(&self) -> Vec<BackgroundTaskEvent> {
        delivery::drain_channel(&self.event_rx, delivery::NOTIFICATION_DELIVERY_BUDGET).items
    }

    fn schedule_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.delivery_wake.clone();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |entity, _cx| {
            // The entity is the sole owner of recurring AI work and its UI bridge.
            entity.runtime.shutdown();
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_deliver = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_deliver {
                    let _ = entity.update(cx, |_entity, cx| {
                        // The workspace consumes both execution requests and state changes
                        // only when this event-driven wake fires.
                        cx.emit(AiBackgroundTaskEvent::DeliveryReady);
                    });
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }
}

impl EventEmitter<AiBackgroundTaskEvent> for AiBackgroundTaskEntity {}

mod view;
pub use view::{AiBackgroundTaskFrame, AiBackgroundTaskView};
