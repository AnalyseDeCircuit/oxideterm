use super::*;
use oxideterm_workspace::delivery as workspace_delivery;

pub(crate) enum HostToolsSamplerDelivery {
    GpuUpdated(GpuUpdate),
    ProfilerChanged,
}

pub(crate) enum HostToolsReliableDelivery {
    // Command output stays inside the Entity-owned delivery queue and never
    // crosses the typed GPUI event boundary or a Debug formatter.
    ProcessAction(HostProcessActionDelivery),
    DockerAction(HostDockerActionDelivery),
    DockerLogs(HostDockerLogsDelivery),
    ContainerCapture(HostContainerCaptureDelivery),
    ContainerAction(HostContainerConfirmDelivery),
    ServiceSnapshot(HostServiceSnapshotDelivery),
    ServiceAction(HostServiceActionDelivery),
    ServiceLogs(HostServiceLogsDelivery),
    TmuxSnapshot(HostTmuxSnapshotDelivery),
    TmuxAction(HostTmuxActionDelivery),
    MuxSnapshot(MuxSnapshotDelivery),
    MuxDetail(MuxDetailDelivery),
    MuxAction(MuxActionDelivery),
    MuxRead(MuxReadDelivery),
    LogSnapshot(HostLogSnapshotDelivery),
    PortSnapshot(HostPortSnapshotDelivery),
    RedisSnapshot(HostRedisSnapshotDelivery),
    RedisKeys(HostRedisKeysDelivery),
    RedisValue(HostRedisValueDelivery),
    RedisMutation(HostRedisMutationDelivery),
    RedisSetValue(HostRedisSetValueDelivery),
    RedisCreateKey(HostRedisCreateKeyDelivery),
    FilesystemSnapshot(HostFilesystemSnapshotDelivery),
    PackageSnapshot(HostPackageSnapshotDelivery),
    ScheduleSnapshot(HostScheduleSnapshotDelivery),
    ScheduleLogs(HostScheduleLogsDelivery),
    ScheduleAction(HostScheduleActionDelivery),
}

impl HostToolsEntity {
    #[cfg(not(target_family = "wasm"))]
    pub(super) fn forward_desktop_updates(
        &self,
        mut profiler_update_rx: tokio::sync::mpsc::Receiver<()>,
        mut gpu_update_rx: tokio::sync::mpsc::UnboundedReceiver<GpuUpdate>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |weak, cx| {
            while profiler_update_rx.recv().await.is_some() {
                // This already runs on the foreground executor; do not enqueue
                // another notification that could accumulate behind slow frames.
                if weak.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();

        let gpu_delivery_tx = self.updates.0.clone();
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
    }

    pub(super) fn schedule_sampler_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.sampler_delivery_wake.clone();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |entity, _| {
            // Releasing the page owner stops sampling shells, never shared SSH nodes.
            entity.visibility = HostToolsVisibility::Dropped;
            entity.stop_page_sampling();
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
                    .update(cx, |entity, cx| entity.drain_sampler_deliveries(cx))
                    .unwrap_or(false);
                if backlog_remaining {
                    // One permit continues the bounded sampler queue without a timer.
                    delivery_wake.mark();
                } else if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_sampler_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = workspace_delivery::drain_channel(
            &self.sampler_delivery_rx,
            workspace_delivery::LIFECYCLE_DELIVERY_BUDGET,
        );
        let active_gpu_connection_id = self.active_gpu_id();
        let mut profiler_changed = false;
        let mut latest_gpu_update = None;

        for delivery in drain.items {
            match delivery {
                HostToolsSamplerDelivery::ProfilerChanged => profiler_changed = true,
                HostToolsSamplerDelivery::GpuUpdated(update)
                    if active_gpu_connection_id == Some(update.connection_id.as_str()) =>
                {
                    // GPU snapshots are latest-value state within one bounded batch.
                    latest_gpu_update = Some(update);
                }
                HostToolsSamplerDelivery::GpuUpdated(_) => {}
            }
        }

        let gpu_updated = latest_gpu_update.is_some();
        if let Some(update) = latest_gpu_update {
            self.host_gpu.snapshot_connection_id = Some(update.connection_id);
            self.host_gpu.snapshot = Some(update.snapshot);
        }
        if gpu_updated || profiler_changed {
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }

    pub(super) fn schedule_reliable_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.reliable_delivery_wake.clone();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // Page results outlive visibility changes but stop with the Entity.
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
                    .update(cx, |entity, cx| entity.drain_reliable_deliveries(cx))
                    .unwrap_or(false);
                if backlog_remaining {
                    delivery_wake.mark();
                } else if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_reliable_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = workspace_delivery::drain_channel(
            &self.reliable_delivery_rx,
            workspace_delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        for delivery in drain.items {
            match delivery {
                HostToolsReliableDelivery::ProcessAction(delivery) => {
                    self.finish_host_process_action(delivery, cx);
                }
                HostToolsReliableDelivery::DockerAction(delivery) => {
                    self.finish_host_docker_action(delivery, cx);
                }
                HostToolsReliableDelivery::DockerLogs(delivery) => {
                    self.finish_host_docker_logs(delivery, cx);
                }
                HostToolsReliableDelivery::ContainerCapture(delivery) => {
                    self.finish_host_container_capture(delivery, cx);
                }
                HostToolsReliableDelivery::ContainerAction(delivery) => {
                    self.finish_host_container_action(delivery, cx);
                }
                HostToolsReliableDelivery::ServiceSnapshot(delivery) => {
                    self.finish_host_service_snapshot(delivery, cx);
                }
                HostToolsReliableDelivery::ServiceAction(delivery) => {
                    self.finish_host_service_action(delivery, cx);
                }
                HostToolsReliableDelivery::ServiceLogs(delivery) => {
                    self.finish_host_service_logs(delivery, cx);
                }
                HostToolsReliableDelivery::TmuxSnapshot(delivery) => {
                    self.finish_host_tmux_snapshot(delivery, cx);
                }
                HostToolsReliableDelivery::TmuxAction(delivery) => {
                    self.finish_host_tmux_action(delivery, cx);
                }
                HostToolsReliableDelivery::MuxSnapshot(delivery) => {
                    self.finish_host_mux_snapshot(delivery, cx);
                }
                HostToolsReliableDelivery::MuxDetail(delivery) => {
                    self.finish_host_mux_detail(delivery, cx);
                }
                HostToolsReliableDelivery::MuxAction(delivery) => {
                    self.finish_host_mux_action(delivery, cx);
                }
                HostToolsReliableDelivery::MuxRead(delivery) => {
                    self.finish_host_mux_read(delivery, cx);
                }
                HostToolsReliableDelivery::LogSnapshot(delivery) => {
                    self.finish_host_logs_snapshot(delivery, cx);
                }
                HostToolsReliableDelivery::PortSnapshot(delivery) => {
                    self.finish_host_ports_snapshot(delivery, cx);
                }
                HostToolsReliableDelivery::RedisSnapshot(delivery) => {
                    self.finish_host_redis_snapshot(delivery, cx);
                }
                HostToolsReliableDelivery::RedisKeys(delivery) => {
                    self.finish_host_redis_keys(delivery, cx);
                }
                HostToolsReliableDelivery::RedisValue(delivery) => {
                    self.finish_host_redis_value(delivery, cx);
                }
                HostToolsReliableDelivery::RedisMutation(delivery) => {
                    self.finish_host_redis_mutation(delivery, cx);
                }
                HostToolsReliableDelivery::RedisSetValue(delivery) => {
                    self.finish_host_redis_set_value(delivery, cx);
                }
                HostToolsReliableDelivery::RedisCreateKey(delivery) => {
                    self.finish_host_redis_create_key(delivery, cx);
                }
                HostToolsReliableDelivery::FilesystemSnapshot(delivery) => {
                    self.finish_host_filesystems_snapshot(delivery, cx);
                }
                HostToolsReliableDelivery::PackageSnapshot(delivery) => {
                    self.finish_host_packages_snapshot(delivery, cx);
                }
                HostToolsReliableDelivery::ScheduleSnapshot(delivery) => {
                    self.finish_host_schedules_snapshot(delivery, cx);
                }
                HostToolsReliableDelivery::ScheduleLogs(delivery) => {
                    self.finish_host_schedule_logs(delivery, cx);
                }
                HostToolsReliableDelivery::ScheduleAction(delivery) => {
                    self.finish_host_schedule_action(delivery, cx);
                }
            }
        }
        drain.outcome.backlog_remaining
    }
}
