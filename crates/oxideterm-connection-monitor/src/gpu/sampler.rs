// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{sync::Arc, time::Duration};

use tokio::{
    runtime::Handle,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::{ResourceSampleShell, ResourceSampler, shell_init_command};

use super::{
    GPU_END_MARKER, GpuSnapshot, GpuSnapshotStatus, GpuUpdate, build_gpu_sample_command,
    parse_gpu_snapshot,
};

pub const GPU_SAMPLE_INTERVAL: Duration = Duration::from_secs(2);
pub const GPU_SAMPLE_TIMEOUT: Duration = Duration::from_secs(10);
pub const GPU_CHANNEL_OPEN_TIMEOUT: Duration = Duration::from_secs(10);
// Multi-accelerator hosts can return sizeable per-process and per-tile data,
// while the cap still prevents an unbounded remote command response.
pub const GPU_MAX_OUTPUT_SIZE: usize = 512 * 1024;

/// Owns one page-scoped GPU sampler and its cancellation path.
pub struct GpuSamplingTask {
    connection_id: String,
    stop_tx: Option<oneshot::Sender<()>>,
    // Retain the worker handle for exactly as long as the page owns sampling.
    _task: JoinHandle<()>,
}

impl GpuSamplingTask {
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub fn is_finished(&self) -> bool {
        self._task.is_finished()
    }

    pub fn stop(mut self) {
        self.request_stop();
    }

    fn request_stop(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
    }
}

impl Drop for GpuSamplingTask {
    fn drop(&mut self) {
        // The worker owns the shell channel. Dropping the page owner requests
        // bounded cleanup without touching the shared SSH connection.
        self.request_stop();
    }
}

pub fn start_gpu_sampling_on(
    connection_id: String,
    sampler: Arc<dyn ResourceSampler>,
    os_type: String,
    update_tx: mpsc::UnboundedSender<GpuUpdate>,
    runtime: Handle,
) -> GpuSamplingTask {
    let (stop_tx, stop_rx) = oneshot::channel();
    let task_connection_id = connection_id.clone();
    let task = runtime.spawn(async move {
        sample_loop(task_connection_id, sampler, os_type, update_tx, stop_rx).await;
    });
    GpuSamplingTask {
        connection_id,
        stop_tx: Some(stop_tx),
        _task: task,
    }
}

async fn sample_loop(
    connection_id: String,
    sampler: Arc<dyn ResourceSampler>,
    os_type: String,
    update_tx: mpsc::UnboundedSender<GpuUpdate>,
    mut stop_rx: oneshot::Receiver<()>,
) {
    if !gpu_os_supported(&os_type) {
        emit_snapshot(
            &update_tx,
            &connection_id,
            GpuSnapshot {
                timestamp_ms: now_ms(),
                status: GpuSnapshotStatus::Unsupported,
                devices: Vec::new(),
                processes: Vec::new(),
            },
        );
        return;
    }

    let mut shell = match open_gpu_shell(sampler.as_ref(), &os_type).await {
        Ok(shell) => shell,
        Err(error) => {
            emit_error(&update_tx, &connection_id, &error);
            return;
        }
    };
    let command = build_gpu_sample_command(&os_type);
    let mut interval = tokio::time::interval(GPU_SAMPLE_INTERVAL);
    // Vendor tools run serially in one registry-owned shell. Skip missed ticks
    // so a slow provider never causes back-to-back catch-up samples.
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = &mut stop_rx => {
                let _ = shell.close().await;
                break;
            }
            _ = interval.tick() => {
                match shell
                    .sample_until(
                        &command,
                        GPU_END_MARKER,
                        GPU_SAMPLE_TIMEOUT,
                        GPU_MAX_OUTPUT_SIZE,
                    )
                    .await
                {
                    Ok(output) => {
                        let snapshot = parse_gpu_snapshot(&output, now_ms());
                        let sampling_complete = matches!(
                            snapshot.status,
                            GpuSnapshotStatus::Unavailable | GpuSnapshotStatus::NoDevices
                        );
                        emit_snapshot(&update_tx, &connection_id, snapshot);
                        if sampling_complete {
                            // Capability absence is stable until the user
                            // explicitly refreshes, so release this shell now.
                            let _ = shell.close().await;
                            break;
                        }
                    }
                    Err(error) => {
                        emit_error(&update_tx, &connection_id, &error);
                        let _ = shell.close().await;
                        match open_gpu_shell(sampler.as_ref(), &os_type).await {
                            Ok(reopened) => shell = reopened,
                            Err(open_error) => {
                                emit_error(&update_tx, &connection_id, &open_error);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn open_gpu_shell(
    sampler: &dyn ResourceSampler,
    os_type: &str,
) -> Result<Box<dyn ResourceSampleShell>, String> {
    // ResourceSampler is backed by the registry-owned node connection. This
    // opens only a managed shell channel and never creates a second transport.
    sampler
        .open_shell(shell_init_command(os_type), GPU_CHANNEL_OPEN_TIMEOUT)
        .await
}

fn gpu_os_supported(os_type: &str) -> bool {
    matches!(
        os_type,
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin"
    )
}

fn emit_error(update_tx: &mpsc::UnboundedSender<GpuUpdate>, connection_id: &str, error: &str) {
    let message = error
        .chars()
        .filter(|character| !character.is_control())
        .take(240)
        .collect::<String>();
    emit_snapshot(
        update_tx,
        connection_id,
        GpuSnapshot {
            timestamp_ms: now_ms(),
            status: GpuSnapshotStatus::Error(message),
            devices: Vec::new(),
            processes: Vec::new(),
        },
    );
}

fn emit_snapshot(
    update_tx: &mpsc::UnboundedSender<GpuUpdate>,
    connection_id: &str,
    snapshot: GpuSnapshot,
) {
    let _ = update_tx.send(GpuUpdate {
        connection_id: connection_id.to_string(),
        snapshot,
    });
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::ResourceSamplerFuture;

    struct TestSampler {
        close_count: Arc<AtomicUsize>,
        sample_count: Arc<AtomicUsize>,
        output: String,
    }

    impl ResourceSampler for TestSampler {
        fn open_shell<'a>(
            &'a self,
            _init_command: &'a str,
            _timeout: Duration,
        ) -> ResourceSamplerFuture<'a, Result<Box<dyn ResourceSampleShell>, String>> {
            let close_count = self.close_count.clone();
            let sample_count = self.sample_count.clone();
            let output = self.output.clone();
            Box::pin(async move {
                Ok(Box::new(TestShell {
                    close_count,
                    sample_count,
                    output,
                }) as Box<dyn ResourceSampleShell>)
            })
        }
    }

    struct TestShell {
        close_count: Arc<AtomicUsize>,
        sample_count: Arc<AtomicUsize>,
        output: String,
    }

    impl ResourceSampleShell for TestShell {
        fn sample_until<'a>(
            &'a mut self,
            _command: &'a str,
            _end_marker: &'a str,
            _timeout: Duration,
            _max_output_size: usize,
        ) -> ResourceSamplerFuture<'a, Result<String, String>> {
            Box::pin(async move {
                self.sample_count.fetch_add(1, Ordering::SeqCst);
                Ok(self.output.clone())
            })
        }

        fn close<'a>(&'a mut self) -> ResourceSamplerFuture<'a, Result<(), String>> {
            Box::pin(async move {
                self.close_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    #[tokio::test]
    async fn stopping_page_task_closes_only_its_shell() {
        let close_count = Arc::new(AtomicUsize::new(0));
        let sample_count = Arc::new(AtomicUsize::new(0));
        let sampler: Arc<dyn ResourceSampler> = Arc::new(TestSampler {
            close_count: close_count.clone(),
            sample_count,
            output: available_gpu_output(),
        });
        let (update_tx, mut update_rx) = mpsc::unbounded_channel();
        let task = start_gpu_sampling_on(
            "connection-a".into(),
            sampler,
            "Linux".into(),
            update_tx,
            Handle::current(),
        );

        let update = tokio::time::timeout(Duration::from_secs(1), update_rx.recv())
            .await
            .expect("GPU sampler should publish promptly")
            .expect("GPU sampler channel should remain open");
        assert_eq!(update.connection_id, "connection-a");

        task.stop();
        tokio::time::timeout(Duration::from_secs(1), async {
            while close_count.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("GPU sampler should close its shell after cancellation");
        assert_eq!(close_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stable_capability_absence_stops_after_one_probe() {
        let cases = [
            (
                "===NVIDIA_STATUS===\nunavailable\n===GPU_NPU_SAMPLE_END===",
                GpuSnapshotStatus::Unavailable,
            ),
            (
                concat!(
                    "===NVIDIA_STATUS===\nunavailable\n",
                    "===AMD_STATUS===\nunavailable\n",
                    "===ASCEND_STATUS===\navailable\n",
                    "===ASCEND_DATA===\nNo devices found\n",
                    "===ASCEND_QUERY_EXIT===\n0\n",
                    "===GPU_NPU_SAMPLE_END==="
                ),
                GpuSnapshotStatus::NoDevices,
            ),
        ];

        for (output, expected_status) in cases {
            let close_count = Arc::new(AtomicUsize::new(0));
            let sample_count = Arc::new(AtomicUsize::new(0));
            let sampler: Arc<dyn ResourceSampler> = Arc::new(TestSampler {
                close_count: close_count.clone(),
                sample_count: sample_count.clone(),
                output: output.into(),
            });
            let (update_tx, mut update_rx) = mpsc::unbounded_channel();
            let task = start_gpu_sampling_on(
                "connection-a".into(),
                sampler,
                "Linux".into(),
                update_tx,
                Handle::current(),
            );

            let update = tokio::time::timeout(Duration::from_secs(1), update_rx.recv())
                .await
                .expect("GPU capability probe should publish promptly")
                .expect("GPU capability probe should publish one snapshot");
            assert_eq!(update.snapshot.status, expected_status);
            tokio::time::timeout(Duration::from_secs(1), async {
                while !task.is_finished() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("stable capability absence should finish its sampling task");

            assert_eq!(sample_count.load(Ordering::SeqCst), 1);
            assert_eq!(close_count.load(Ordering::SeqCst), 1);
        }
    }

    fn available_gpu_output() -> String {
        concat!(
            "===NVIDIA_STATUS===\navailable\n",
            "===NVIDIA_GPUS===\n",
            "0, GPU-a, 00000000:01:00.0, NVIDIA L40S, 555.42, P0, 10, 2, 512, 46068, 41, 50, 350, N/A\n",
            "===NVIDIA_GPU_QUERY_EXIT===\n0\n",
            "===NVIDIA_PROCESSES===\n",
            "===NVIDIA_GPU_END==="
        )
        .to_string()
    }
}
