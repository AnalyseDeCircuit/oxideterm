// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;

use async_trait::async_trait;
use gpui::{Context, EventEmitter};
use oxideterm_ai_tasks::{
    BackgroundTaskEvent, BackgroundTaskExecution, BackgroundTaskExecutionResult,
    BackgroundTaskExecutor, BackgroundTaskId, BackgroundTaskRuntime, BackgroundTaskSnapshot,
    BackgroundTaskSpec, BackgroundTaskValidationError,
};

use super::delivery;

pub(in crate::workspace) struct AiBackgroundExecutionRequest {
    pub(in crate::workspace) execution: BackgroundTaskExecution,
    pub(in crate::workspace) response:
        tokio::sync::oneshot::Sender<Result<BackgroundTaskExecutionResult, String>>,
}

struct ApplicationBackgroundExecutor {
    request_tx: delivery::ActiveDeliverySender<AiBackgroundExecutionRequest>,
}

#[async_trait]
impl BackgroundTaskExecutor for ApplicationBackgroundExecutor {
    async fn execute(
        &self,
        execution: BackgroundTaskExecution,
    ) -> Result<BackgroundTaskExecutionResult, String> {
        let (response, receiver) = tokio::sync::oneshot::channel();
        self.request_tx
            .send(AiBackgroundExecutionRequest {
                execution,
                response,
            })
            .map_err(|_| "The OxideSens background task owner has closed.".to_string())?;
        receiver
            .await
            .map_err(|_| "The OxideSens background task execution was cancelled.".to_string())?
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AiBackgroundTaskEvent {
    DeliveryReady,
}

pub(in crate::workspace) struct AiBackgroundTaskEntity {
    runtime: BackgroundTaskRuntime,
    delivery_wake: delivery::ActiveDeliveryWake,
    request_rx: std::sync::mpsc::Receiver<AiBackgroundExecutionRequest>,
    event_rx: std::sync::mpsc::Receiver<BackgroundTaskEvent>,
    event_forwarder: tokio::task::AbortHandle,
}

impl AiBackgroundTaskEntity {
    pub(in crate::workspace) fn new(
        task_runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> Self {
        let delivery_wake = delivery::ActiveDeliveryWake::default();
        let (request_tx, request_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(delivery_wake.clone());
        let executor = Arc::new(ApplicationBackgroundExecutor { request_tx });
        let (runtime, mut core_event_rx) =
            BackgroundTaskRuntime::new(executor, task_runtime.handle().clone());
        let (event_tx, event_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(delivery_wake.clone());
        let event_forwarder = task_runtime
            .spawn(async move {
                while let Some(event) = core_event_rx.recv().await {
                    if event_tx.send(event).is_err() {
                        break;
                    }
                }
            })
            .abort_handle();
        let entity = Self {
            runtime,
            delivery_wake,
            request_rx,
            event_rx,
            event_forwarder,
        };
        entity.schedule_delivery(cx);
        entity
    }

    pub(in crate::workspace) fn create(
        &self,
        spec: BackgroundTaskSpec,
    ) -> Result<BackgroundTaskId, BackgroundTaskValidationError> {
        self.runtime.create(spec)
    }

    pub(in crate::workspace) fn snapshots_for_owner(
        &self,
        conversation_id: &str,
    ) -> Vec<BackgroundTaskSnapshot> {
        self.runtime.snapshots_for_owner(conversation_id)
    }

    pub(in crate::workspace) fn snapshot_for_owner(
        &self,
        conversation_id: &str,
        task_id: &BackgroundTaskId,
    ) -> Option<BackgroundTaskSnapshot> {
        self.runtime
            .snapshot(task_id)
            .filter(|snapshot| snapshot.owner.conversation_id == conversation_id)
    }

    pub(in crate::workspace) fn cancel_for_owner(
        &self,
        conversation_id: &str,
        task_id: &BackgroundTaskId,
    ) -> bool {
        self.snapshot_for_owner(conversation_id, task_id)
            .is_some_and(|_| self.runtime.cancel(task_id))
    }

    pub(in crate::workspace) fn cancel_owner(&self, conversation_id: &str) -> usize {
        self.runtime.cancel_owner(conversation_id)
    }

    pub(in crate::workspace) fn cancel_all(&self) -> usize {
        self.runtime.cancel_all()
    }

    pub(in crate::workspace) fn take_execution_requests(
        &self,
    ) -> Vec<AiBackgroundExecutionRequest> {
        delivery::drain_channel(&self.request_rx, delivery::USER_ACTION_DELIVERY_BUDGET).items
    }

    pub(in crate::workspace) fn take_events(&self) -> Vec<BackgroundTaskEvent> {
        delivery::drain_channel(&self.event_rx, delivery::NOTIFICATION_DELIVERY_BUDGET).items
    }

    fn schedule_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.delivery_wake.clone();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |entity, _cx| {
            // The entity is the sole owner of recurring AI work and its UI bridge.
            entity.runtime.shutdown();
            entity.event_forwarder.abort();
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
