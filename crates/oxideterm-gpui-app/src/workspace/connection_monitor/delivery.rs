use super::*;
use crate::workspace::delivery as workspace_delivery;

const HOST_TOOLS_RESULT_RECEIVER_COUNT: usize = 15;

pub(super) enum HostToolsSamplerDelivery {
    ProfilerUpdated,
    GpuUpdated(GpuUpdate),
}

pub(in crate::workspace) struct HostToolsDeliveryBridges {
    pub(super) profiler_update_rx: tokio::sync::mpsc::UnboundedReceiver<ProfilerUpdate>,
    pub(super) gpu_update_rx: tokio::sync::mpsc::UnboundedReceiver<GpuUpdate>,
    pub(super) sampler_delivery_tx:
        workspace_delivery::ActiveDeliverySender<HostToolsSamplerDelivery>,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn schedule_host_tools_delivery(
        &self,
        bridges: HostToolsDeliveryBridges,
        cx: &mut Context<Self>,
    ) {
        let HostToolsDeliveryBridges {
            mut profiler_update_rx,
            mut gpu_update_rx,
            sampler_delivery_tx,
        } = bridges;
        let profiler_delivery_tx = sampler_delivery_tx.clone();
        cx.spawn(async move |_, _| {
            while profiler_update_rx.recv().await.is_some() {
                // ProfilerRegistry already owns the snapshot; only its change signal crosses
                // into the foreground queue.
                if profiler_delivery_tx
                    .send(HostToolsSamplerDelivery::ProfilerUpdated)
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        let gpu_delivery_tx = sampler_delivery_tx;
        cx.spawn(async move |_, _| {
            while let Some(update) = gpu_update_rx.recv().await {
                if gpu_delivery_tx
                    .send(HostToolsSamplerDelivery::GpuUpdated(update))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        let delivery_wake = self.connection_monitor.delivery_wake.clone();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // Sampling runtimes stop through their owners; release only ends this UI waiter.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |weak, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if !should_drain {
                    if stopped {
                        break;
                    }
                    continue;
                }
                let backlog_remaining = weak
                    .update(cx, |workspace, cx| workspace.poll_host_tools_deliveries(cx))
                    .unwrap_or(false);
                if backlog_remaining {
                    // One permit continues every shared Host Tools queue without a timer.
                    delivery_wake.mark();
                } else if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_host_tools_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let sampler_backlog = self.poll_host_tools_sampler_deliveries(cx);
        let result_backlog = self.poll_host_tools_result_receivers(cx);
        sampler_backlog || result_backlog
    }

    fn poll_host_tools_sampler_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = workspace_delivery::drain_channel(
            &self.connection_monitor.sampler_delivery_rx,
            workspace_delivery::LIFECYCLE_DELIVERY_BUDGET,
        );
        let active_gpu_connection_id = self
            .connection_monitor
            .host_gpu
            .sampling_task
            .as_ref()
            .map(|task| task.connection_id().to_string());
        let mut profiler_updated = false;
        let mut latest_gpu_update = None;

        for delivery in drain.items {
            match delivery {
                HostToolsSamplerDelivery::ProfilerUpdated => profiler_updated = true,
                HostToolsSamplerDelivery::GpuUpdated(update)
                    if active_gpu_connection_id.as_deref()
                        == Some(update.connection_id.as_str()) =>
                {
                    // GPU snapshots are latest-value state within one bounded batch.
                    latest_gpu_update = Some(update);
                }
                HostToolsSamplerDelivery::GpuUpdated(_) => {}
            }
        }

        let gpu_updated = latest_gpu_update.is_some();
        if let Some(update) = latest_gpu_update {
            self.connection_monitor.host_gpu.snapshot_connection_id = Some(update.connection_id);
            self.connection_monitor.host_gpu.snapshot = Some(update.snapshot);
        }
        if profiler_updated || gpu_updated {
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }

    fn poll_host_tools_result_receivers(&mut self, cx: &mut Context<Self>) -> bool {
        let started_at = Instant::now();
        let mut checked = 0usize;

        while checked < HOST_TOOLS_RESULT_RECEIVER_COUNT
            && workspace_delivery::USER_ACTION_DELIVERY_BUDGET
                .allows_next(checked, started_at.elapsed())
        {
            match self.connection_monitor.delivery_cursor {
                0 => self.poll_host_process_action_results(cx),
                1 => self.poll_host_docker_action_results(cx),
                2 => self.poll_host_docker_logs_results(cx),
                3 => self.poll_host_service_action_results(cx),
                4 => self.poll_host_service_snapshot_results(cx),
                5 => self.poll_host_service_logs_results(cx),
                6 => self.poll_host_logs_snapshot_results(cx),
                7 => self.poll_host_tmux_snapshot_results(cx),
                8 => self.poll_host_tmux_action_results(cx),
                9 => self.poll_host_ports_snapshot_results(cx),
                10 => self.poll_host_schedules_snapshot_results(cx),
                11 => self.poll_host_filesystems_snapshot_results(cx),
                12 => self.poll_host_packages_snapshot_results(cx),
                13 => self.poll_host_schedule_logs_results(cx),
                14 => self.poll_host_schedule_action_results(cx),
                _ => unreachable!("Host Tools delivery cursor must stay within receiver count"),
            }
            self.connection_monitor.delivery_cursor =
                (self.connection_monitor.delivery_cursor + 1) % HOST_TOOLS_RESULT_RECEIVER_COUNT;
            checked += 1;
        }

        // An incomplete fair scan may have skipped the receiver that caused this wake.
        checked < HOST_TOOLS_RESULT_RECEIVER_COUNT
    }
}
