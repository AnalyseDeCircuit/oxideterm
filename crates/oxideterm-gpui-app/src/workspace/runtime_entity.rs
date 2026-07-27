// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_ssh::ReconnectTiming;

const ACTIVE_PROBE_START_DELAY: Duration = Duration::from_millis(530);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum WorkspaceRuntimeEvent {
    WorkerResultsReady,
    ActiveConnectionsChanged,
}

/// Owns runtime worker endpoints and reliable delivery independently from tabs.
pub(in crate::workspace) struct WorkspaceRuntimeEntity {
    ssh_worker_tx: delivery::ActiveDeliverySender<SshConnectionWorkerResult>,
    ssh_worker_rx: std::sync::mpsc::Receiver<SshConnectionWorkerResult>,
    reconnect_worker_tx: delivery::ActiveDeliverySender<ReconnectWorkerResult>,
    reconnect_worker_rx: std::sync::mpsc::Receiver<ReconnectWorkerResult>,
    active_probe_tx: delivery::ActiveDeliverySender<usize>,
    active_probe_rx: std::sync::mpsc::Receiver<usize>,
    ssh_results: VecDeque<SshConnectionWorkerResult>,
    reconnect_results: VecDeque<ReconnectWorkerResult>,
    ssh_registry: SshConnectionRegistry,
    task_runtime: Arc<tokio::runtime::Runtime>,
    reconnect_timing: ReconnectTiming,
    ssh_active_probe_in_flight: bool,
    active_probe_timer_generation: u64,
}

impl WorkspaceRuntimeEntity {
    pub(in crate::workspace) fn new(
        ssh_registry: SshConnectionRegistry,
        task_runtime: Arc<tokio::runtime::Runtime>,
        reconnect_timing: ReconnectTiming,
        cx: &mut Context<Self>,
    ) -> Self {
        let runtime_wake = delivery::ActiveDeliveryWake::default();
        let (ssh_worker_tx, ssh_worker_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_wake.clone());
        let (reconnect_worker_tx, reconnect_worker_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_wake.clone());
        let (active_probe_tx, active_probe_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_wake);
        let mut entity = Self {
            ssh_worker_tx,
            ssh_worker_rx,
            reconnect_worker_tx,
            reconnect_worker_rx,
            active_probe_tx,
            active_probe_rx,
            ssh_results: VecDeque::new(),
            reconnect_results: VecDeque::new(),
            ssh_registry,
            task_runtime,
            reconnect_timing,
            ssh_active_probe_in_flight: false,
            active_probe_timer_generation: 0,
        };
        entity.schedule_worker_delivery(cx);
        entity.schedule_active_probe_after(ACTIVE_PROBE_START_DELAY, cx);
        entity
    }

    pub(in crate::workspace) fn configure_reconnect_timing(
        &mut self,
        reconnect_timing: ReconnectTiming,
        cx: &mut Context<Self>,
    ) {
        self.reconnect_timing = reconnect_timing;
        self.schedule_active_probe_after(ACTIVE_PROBE_START_DELAY, cx);
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

    fn schedule_active_probe_after(&mut self, delay: Duration, cx: &mut Context<Self>) {
        self.active_probe_timer_generation = self.active_probe_timer_generation.wrapping_add(1);
        let generation = self.active_probe_timer_generation;
        cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                if entity.active_probe_timer_generation == generation {
                    entity.start_active_ssh_probe(cx);
                }
            });
        })
        .detach();
    }

    fn start_active_ssh_probe(&mut self, cx: &mut Context<Self>) {
        if self.ssh_active_probe_in_flight {
            // A settings change can reschedule while a previous probe is still running.
            self.schedule_active_probe_after(ACTIVE_PROBE_START_DELAY, cx);
            return;
        }
        let registry_stats = self.ssh_registry.stats();
        if registry_stats.active == 0 && registry_stats.idle == 0 {
            self.schedule_active_probe_after(self.reconnect_timing.ssh_keepalive_interval, cx);
            return;
        }

        self.ssh_active_probe_in_flight = true;
        let ssh_registry = self.ssh_registry.clone();
        let timeout = self.reconnect_timing.proactive_keepalive_timeout;
        let result_sender = self.active_probe_tx.clone();
        self.task_runtime.spawn(async move {
            let changed = ssh_registry.probe_active_connections(timeout).await.len();
            let _ = result_sender.send(changed);
        });
    }

    fn drain_worker_results(&mut self, cx: &mut Context<Self>) -> bool {
        let ssh_batch =
            delivery::drain_channel(&self.ssh_worker_rx, delivery::USER_ACTION_DELIVERY_BUDGET);
        let reconnect_batch = delivery::drain_channel(
            &self.reconnect_worker_rx,
            delivery::LIFECYCLE_DELIVERY_BUDGET,
        );
        let active_probe_batch =
            delivery::drain_channel(&self.active_probe_rx, delivery::LIFECYCLE_DELIVERY_BUDGET);
        let received_ssh_results = !ssh_batch.items.is_empty();
        self.ssh_results.extend(ssh_batch.items);
        let received_reconnect_results = !reconnect_batch.items.is_empty();
        self.reconnect_results.extend(reconnect_batch.items);
        let active_probe_completed = !active_probe_batch.items.is_empty();
        let active_connections_changed = active_probe_batch
            .items
            .into_iter()
            .any(|changed| changed > 0);
        if active_probe_completed {
            self.ssh_active_probe_in_flight = false;
            self.schedule_active_probe_after(self.reconnect_timing.ssh_keepalive_interval, cx);
        }
        if received_ssh_results || received_reconnect_results {
            cx.emit(WorkspaceRuntimeEvent::WorkerResultsReady);
        }
        if active_connections_changed {
            cx.emit(WorkspaceRuntimeEvent::ActiveConnectionsChanged);
        }
        ssh_batch.outcome.backlog_remaining
            || reconnect_batch.outcome.backlog_remaining
            || active_probe_batch.outcome.backlog_remaining
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
    use std::sync::atomic::{AtomicBool, Ordering};

    fn test_task_runtime() -> Arc<tokio::runtime::Runtime> {
        Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime"),
        )
    }

    fn test_runtime_entity(cx: &mut TestAppContext) -> Entity<WorkspaceRuntimeEntity> {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let task_runtime = test_task_runtime();
        cx.new(|cx| {
            WorkspaceRuntimeEntity::new(ssh_registry, task_runtime, ReconnectTiming::default(), cx)
        })
    }

    #[gpui::test]
    fn worker_results_and_release_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
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

    #[gpui::test]
    fn active_probe_completion_stays_inside_runtime_entity(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let changed_event_seen = Arc::new(AtomicBool::new(false));
        let changed_event_flag = changed_event_seen.clone();
        let _event_subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &WorkspaceRuntimeEvent, _cx| {
                if *event == WorkspaceRuntimeEvent::ActiveConnectionsChanged {
                    changed_event_flag.store(true, Ordering::Release);
                }
            })
        });
        let active_probe_sender = entity.update(cx, |entity, _cx| {
            entity.ssh_active_probe_in_flight = true;
            entity.active_probe_tx.clone()
        });
        active_probe_sender
            .send(0)
            .expect("active probe completion");

        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            let (_ssh_results, reconnect_results) = entity.take_worker_results();
            assert!(reconnect_results.is_empty());
            assert!(!entity.ssh_active_probe_in_flight);
        });
        assert!(!changed_event_seen.load(Ordering::Acquire));

        entity.update(cx, |entity, _cx| {
            entity.ssh_active_probe_in_flight = true;
        });
        active_probe_sender.send(1).expect("changed active probe");
        cx.run_until_parked();
        assert!(changed_event_seen.load(Ordering::Acquire));
    }

    #[gpui::test]
    fn empty_registry_and_timing_changes_reschedule_without_probe(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let initial_generation = cx.read(|cx| entity.read(cx).active_probe_timer_generation);

        entity.update(cx, |entity, cx| {
            entity.start_active_ssh_probe(cx);
            assert!(!entity.ssh_active_probe_in_flight);
            assert!(entity.active_probe_timer_generation > initial_generation);

            let mut reconnect_timing = ReconnectTiming::default();
            reconnect_timing.ssh_keepalive_interval = Duration::from_secs(37);
            entity.configure_reconnect_timing(reconnect_timing, cx);
            assert_eq!(
                entity.reconnect_timing.ssh_keepalive_interval,
                Duration::from_secs(37)
            );
        });
    }

    #[gpui::test]
    fn entity_release_preserves_registry_consumers(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_consumer = ConnectionConsumer::NodeRouter("test-node".to_string());
        let connection_handle = ssh_registry.acquire(SshConfig::default(), node_consumer.clone());
        let connection_id = connection_handle.connection_id().to_string();
        let entity_registry = ssh_registry.clone();
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                entity_registry,
                task_runtime,
                ReconnectTiming::default(),
                cx,
            )
        });

        drop(entity);
        cx.update(|_cx| {});

        let connection_info = ssh_registry
            .get(&connection_id)
            .expect("runtime connection remains registered")
            .info();
        assert_eq!(connection_info.ref_count, 1);
        assert_eq!(connection_info.consumers, vec![node_consumer]);
    }
}
