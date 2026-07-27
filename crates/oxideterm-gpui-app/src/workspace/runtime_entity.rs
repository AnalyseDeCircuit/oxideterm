// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(in crate::workspace) enum WorkspaceRuntimeEvent {
    WorkerResultsReady,
}

/// Owns runtime worker endpoints and reliable delivery independently from tabs.
pub(in crate::workspace) struct WorkspaceRuntimeEntity {
    ssh_worker_tx: delivery::ActiveDeliverySender<SshConnectionWorkerResult>,
    ssh_worker_rx: std::sync::mpsc::Receiver<SshConnectionWorkerResult>,
    reconnect_worker_tx: delivery::ActiveDeliverySender<ReconnectWorkerResult>,
    reconnect_worker_rx: std::sync::mpsc::Receiver<ReconnectWorkerResult>,
    ssh_results: VecDeque<SshConnectionWorkerResult>,
    reconnect_results: VecDeque<ReconnectWorkerResult>,
}

impl WorkspaceRuntimeEntity {
    pub(in crate::workspace) fn new(cx: &mut Context<Self>) -> Self {
        let runtime_wake = delivery::ActiveDeliveryWake::default();
        let (ssh_worker_tx, ssh_worker_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_wake.clone());
        let (reconnect_worker_tx, reconnect_worker_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_wake);
        let entity = Self {
            ssh_worker_tx,
            ssh_worker_rx,
            reconnect_worker_tx,
            reconnect_worker_rx,
            ssh_results: VecDeque::new(),
            reconnect_results: VecDeque::new(),
        };
        entity.schedule_worker_delivery(cx);
        entity
    }

    pub(in crate::workspace) fn ssh_worker_sender(
        &self,
    ) -> delivery::ActiveDeliverySender<SshConnectionWorkerResult> {
        // Worker tasks need only a shallow channel endpoint, never runtime state.
        self.ssh_worker_tx.clone()
    }

    pub(in crate::workspace) fn reconnect_worker_sender(
        &self,
    ) -> delivery::ActiveDeliverySender<ReconnectWorkerResult> {
        // Delayed reconnect phases keep the Entity wake path without a root sender field.
        self.reconnect_worker_tx.clone()
    }

    pub(in crate::workspace) fn take_worker_results(
        &mut self,
    ) -> (
        VecDeque<SshConnectionWorkerResult>,
        VecDeque<ReconnectWorkerResult>,
    ) {
        (
            std::mem::take(&mut self.ssh_results),
            std::mem::take(&mut self.reconnect_results),
        )
    }

    fn schedule_worker_delivery(&self, cx: &mut Context<Self>) {
        let runtime_wake = self.ssh_worker_tx.wake();
        let release_wake = runtime_wake.clone();
        cx.on_release(move |_, _| {
            // Workspace release stops UI delivery, not in-flight backend work.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                runtime_wake.wait().await;
                let should_drain = runtime_wake.take();
                let stopped = runtime_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_worker_results(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        runtime_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_worker_results(&mut self, cx: &mut Context<Self>) -> bool {
        let ssh_batch =
            delivery::drain_channel(&self.ssh_worker_rx, delivery::USER_ACTION_DELIVERY_BUDGET);
        let reconnect_batch = delivery::drain_channel(
            &self.reconnect_worker_rx,
            delivery::LIFECYCLE_DELIVERY_BUDGET,
        );
        let received_results = !ssh_batch.items.is_empty() || !reconnect_batch.items.is_empty();
        self.ssh_results.extend(ssh_batch.items);
        self.reconnect_results.extend(reconnect_batch.items);
        if received_results {
            cx.emit(WorkspaceRuntimeEvent::WorkerResultsReady);
        }
        ssh_batch.outcome.backlog_remaining || reconnect_batch.outcome.backlog_remaining
    }
}

impl gpui::EventEmitter<WorkspaceRuntimeEvent> for WorkspaceRuntimeEntity {}

impl WorkspaceApp {
    pub(in crate::workspace) fn ssh_worker_sender(
        &self,
        cx: &App,
    ) -> delivery::ActiveDeliverySender<SshConnectionWorkerResult> {
        self.workspace_runtime.read(cx).ssh_worker_sender()
    }

    pub(in crate::workspace) fn reconnect_worker_sender(
        &self,
        cx: &App,
    ) -> delivery::ActiveDeliverySender<ReconnectWorkerResult> {
        self.workspace_runtime.read(cx).reconnect_worker_sender()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn worker_results_and_release_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(WorkspaceRuntimeEntity::new);
        let (ssh_tx, reconnect_tx, wake) = cx.read(|cx| {
            let entity = entity.read(cx);
            (
                entity.ssh_worker_sender(),
                entity.reconnect_worker_sender(),
                entity.ssh_worker_tx.wake(),
            )
        });
        ssh_tx
            .send(SshConnectionWorkerResult::Test { result: Ok(()) })
            .expect("SSH worker delivery");
        reconnect_tx
            .send(ReconnectWorkerResult::ContinueReconnectCascade)
            .expect("reconnect worker delivery");

        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            let (ssh_results, reconnect_results) = entity.take_worker_results();
            assert!(matches!(
                ssh_results.front(),
                Some(SshConnectionWorkerResult::Test { result: Ok(()) })
            ));
            assert!(matches!(
                reconnect_results.front(),
                Some(ReconnectWorkerResult::ContinueReconnectCascade)
            ));
        });

        drop(entity);
        cx.update(|_cx| {});
        assert!(wake.is_stopped());
    }
}
