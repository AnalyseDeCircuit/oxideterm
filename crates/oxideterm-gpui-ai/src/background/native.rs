use std::sync::Arc;

use super::*;
use async_trait::async_trait;
use oxideterm_ai_tasks::{
    BackgroundTaskExecution, BackgroundTaskExecutionResult, BackgroundTaskExecutor,
    BackgroundTaskRuntime,
};

pub struct AiBackgroundExecutionRequest {
    pub execution: BackgroundTaskExecution,
    pub response: tokio::sync::oneshot::Sender<Result<BackgroundTaskExecutionResult, String>>,
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

struct DesktopTasks {
    runtime: BackgroundTaskRuntime,
    forwarder: tokio::task::AbortHandle,
}

impl AiBackgroundHost for DesktopTasks {
    fn create(
        &self,
        spec: BackgroundTaskSpec,
    ) -> Result<BackgroundTaskId, BackgroundTaskValidationError> {
        self.runtime.create(spec)
    }
    fn snapshots_for_owner(&self, conversation_id: &str) -> Vec<BackgroundTaskSnapshot> {
        self.runtime.snapshots_for_owner(conversation_id)
    }
    fn snapshot(&self, id: &BackgroundTaskId) -> Option<BackgroundTaskSnapshot> {
        self.runtime.snapshot(id)
    }
    fn cancel(&self, id: &BackgroundTaskId) -> bool {
        self.runtime.cancel(id)
    }
    fn cancel_owner(&self, conversation_id: &str) -> usize {
        self.runtime.cancel_owner(conversation_id)
    }
    fn cancel_all(&self) -> usize {
        self.runtime.cancel_all()
    }
    fn shutdown(&self) {
        self.runtime.shutdown();
        self.forwarder.abort();
    }
}

impl AiBackgroundTaskEntity {
    pub fn new(task_runtime: Arc<tokio::runtime::Runtime>, cx: &mut Context<Self>) -> Self {
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
            runtime: Box::new(DesktopTasks {
                runtime,
                forwarder: event_forwarder,
            }),
            delivery_wake,
            request_rx: Some(request_rx),
            event_rx,
        };
        entity.schedule_delivery(cx);
        entity
    }
}
