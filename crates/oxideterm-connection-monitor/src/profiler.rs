// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tokio::{
    runtime::Handle,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::{
    MetricsSource, PreviousResourceSample, RESOURCE_HISTORY_CAPACITY, ResourceMetrics,
    parse_cpu_snapshot, parse_net_snapshot, parse_resource_metrics, push_history,
};

pub const RESOURCE_SAMPLE_INTERVAL: Duration = Duration::from_secs(10);
pub const RESOURCE_SAMPLE_TIMEOUT: Duration = Duration::from_secs(5);
pub const RESOURCE_CHANNEL_OPEN_TIMEOUT: Duration = Duration::from_secs(10);
pub const RESOURCE_MAX_OUTPUT_SIZE: usize = 65_536;
pub const RESOURCE_MAX_CONSECUTIVE_FAILURES: u32 = 3;
pub const RESOURCE_END_MARKER: &str = "===END===";

const METRICS_COMMAND_LINUX: &str = "echo '===STAT==='; head -1 /proc/stat 2>/dev/null; echo '===MEMINFO==='; grep -E '^(MemTotal|MemAvailable):' /proc/meminfo 2>/dev/null; echo '===LOADAVG==='; cat /proc/loadavg 2>/dev/null; echo '===NETDEV==='; cat /proc/net/dev 2>/dev/null; echo '===NPROC==='; nproc 2>/dev/null";
const PORT_CMD_LINUX: &str = "echo '===PORTS==='; ((ss -tlnp 2>/dev/null || netstat -tlnp 2>/dev/null) | grep -i listen || true); echo '===PORTS_END==='; echo '===DOCKER==='; ((docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}' 2>/dev/null || sudo -n docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}' 2>/dev/null) || true); echo '===DOCKER_END==='";
const PORT_CMD_MACOS: &str = "echo '===PORTS==='; ((lsof -iTCP -sTCP:LISTEN -nP 2>/dev/null | tail -n +2) || true); echo '===PORTS_END==='; echo '===DOCKER==='; ((docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}' 2>/dev/null || sudo -n docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}' 2>/dev/null) || true); echo '===DOCKER_END==='";
const PORT_CMD_WINDOWS: &str = "echo '===PORTS==='; powershell -NoProfile -Command \"Get-NetTCPConnection -State Listen 2>$null | Select-Object LocalAddress,LocalPort,OwningProcess | Format-Table -HideTableHeaders\" 2>/dev/null; echo '===PORTS_END==='";
const PORT_CMD_FREEBSD: &str =
    "echo '===PORTS==='; sockstat -4 -6 -l -P tcp 2>/dev/null | tail -n +2; echo '===PORTS_END==='";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfilerState {
    Running,
    #[default]
    Stopped,
    Degraded,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilerUpdate {
    pub connection_id: String,
    pub metrics: ResourceMetrics,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConnectionProfilerSnapshot {
    pub metrics: Option<ResourceMetrics>,
    pub history: Vec<ResourceMetrics>,
    pub state: ProfilerState,
}

pub type ResourceSamplerFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait ResourceSampleShell: Send {
    fn sample_until<'a>(
        &'a mut self,
        command: &'a str,
        end_marker: &'a str,
        timeout: Duration,
        max_output_size: usize,
    ) -> ResourceSamplerFuture<'a, Result<String, String>>;

    fn close<'a>(&'a mut self) -> ResourceSamplerFuture<'a, Result<(), String>>;
}

pub trait ResourceSampler: Send + Sync + 'static {
    fn open_shell<'a>(
        &'a self,
        init_command: &'a str,
        timeout: Duration,
    ) -> ResourceSamplerFuture<'a, Result<Box<dyn ResourceSampleShell>, String>>;
}

struct ConnectionProfilerEntry {
    snapshot: ConnectionProfilerSnapshot,
    stop_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

#[derive(Clone, Default)]
pub struct ProfilerRegistry {
    profilers: Arc<Mutex<HashMap<String, ConnectionProfilerEntry>>>,
}

impl ProfilerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start is Tauri-compatible: running profilers are idempotent, while
    /// stopped/degraded entries are dropped and recreated with empty history.
    pub fn start(&self, connection_id: impl Into<String>) -> bool {
        let connection_id = connection_id.into();
        let mut profilers = lock(&self.profilers);
        if matches!(
            profilers
                .get(&connection_id)
                .map(|entry| entry.snapshot.state),
            Some(ProfilerState::Running)
        ) {
            return false;
        }

        profilers.insert(
            connection_id,
            ConnectionProfilerEntry {
                snapshot: running_snapshot(),
                stop_tx: None,
                task: None,
            },
        );
        true
    }

    pub fn start_with_sampler(
        &self,
        connection_id: impl Into<String>,
        sampler: Arc<dyn ResourceSampler>,
        os_type: impl Into<String>,
        update_tx: Option<mpsc::UnboundedSender<ProfilerUpdate>>,
    ) -> bool {
        let spawn_handle = Handle::try_current().ok();
        self.start_with_sampler_on_handle(connection_id, sampler, os_type, update_tx, spawn_handle)
    }

    pub fn start_with_sampler_on(
        &self,
        connection_id: impl Into<String>,
        sampler: Arc<dyn ResourceSampler>,
        os_type: impl Into<String>,
        update_tx: Option<mpsc::UnboundedSender<ProfilerUpdate>>,
        handle: Handle,
    ) -> bool {
        self.start_with_sampler_on_handle(connection_id, sampler, os_type, update_tx, Some(handle))
    }

    fn start_with_sampler_on_handle(
        &self,
        connection_id: impl Into<String>,
        sampler: Arc<dyn ResourceSampler>,
        os_type: impl Into<String>,
        update_tx: Option<mpsc::UnboundedSender<ProfilerUpdate>>,
        spawn_handle: Option<Handle>,
    ) -> bool {
        let connection_id = connection_id.into();
        let os_type = os_type.into();
        let (stop_tx, stop_rx) = oneshot::channel();
        {
            let mut profilers = lock(&self.profilers);
            if matches!(
                profilers
                    .get(&connection_id)
                    .map(|entry| entry.snapshot.state),
                Some(ProfilerState::Running)
            ) {
                return false;
            }
            if let Some(mut previous) = profilers.remove(&connection_id) {
                if let Some(stop_tx) = previous.stop_tx.take() {
                    let _ = stop_tx.send(());
                }
            }
            profilers.insert(
                connection_id.clone(),
                ConnectionProfilerEntry {
                    snapshot: running_snapshot(),
                    stop_tx: Some(stop_tx),
                    task: None,
                },
            );
        }

        let registry = self.clone();
        let task_connection_id = connection_id.clone();
        let task_future = async move {
            sample_loop(
                registry,
                task_connection_id,
                sampler,
                os_type,
                update_tx,
                stop_rx,
            )
            .await;
        };

        if let Some(handle) = spawn_handle {
            let task = handle.spawn(task_future);
            if let Some(entry) = lock(&self.profilers).get_mut(&connection_id) {
                entry.task = Some(task);
            }
        } else {
            spawn_profiler_thread(task_future);
        }
        true
    }

    pub fn stop(&self, connection_id: &str) -> bool {
        let Some(mut entry) = lock(&self.profilers).remove(connection_id) else {
            return false;
        };
        if let Some(stop_tx) = entry.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        true
    }

    pub fn remove(&self, connection_id: &str) -> bool {
        self.stop(connection_id)
    }

    pub fn stop_all(&self) {
        let keys = lock(&self.profilers).keys().cloned().collect::<Vec<_>>();
        for key in keys {
            self.stop(&key);
        }
    }

    pub fn mark_degraded(&self, connection_id: &str) -> bool {
        let mut profilers = lock(&self.profilers);
        let Some(entry) = profilers.get_mut(connection_id) else {
            return false;
        };
        entry.snapshot.state = ProfilerState::Degraded;
        true
    }

    pub fn record_metrics(&self, update: ProfilerUpdate) -> bool {
        let mut profilers = lock(&self.profilers);
        let Some(entry) = profilers.get_mut(&update.connection_id) else {
            return false;
        };
        entry.snapshot.metrics = Some(update.metrics.clone());
        push_history(&mut entry.snapshot.history, update.metrics);
        true
    }

    pub fn latest(&self, connection_id: &str) -> Option<ResourceMetrics> {
        lock(&self.profilers)
            .get(connection_id)
            .and_then(|entry| entry.snapshot.metrics.clone())
    }

    pub fn history(&self, connection_id: &str) -> Vec<ResourceMetrics> {
        lock(&self.profilers)
            .get(connection_id)
            .map(|entry| entry.snapshot.history.clone())
            .unwrap_or_default()
    }

    pub fn state(&self, connection_id: &str) -> Option<ProfilerState> {
        lock(&self.profilers)
            .get(connection_id)
            .map(|entry| entry.snapshot.state)
    }

    pub fn snapshot(&self, connection_id: &str) -> Option<ConnectionProfilerSnapshot> {
        lock(&self.profilers)
            .get(connection_id)
            .map(|entry| entry.snapshot.clone())
    }

    pub fn connection_ids(&self) -> Vec<String> {
        lock(&self.profilers).keys().cloned().collect()
    }
}

pub fn build_sample_command(os_type: &str) -> String {
    let metrics = METRICS_COMMAND_LINUX;
    let port_cmd = match os_type {
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => PORT_CMD_LINUX,
        "macOS" | "macos" | "Darwin" => PORT_CMD_MACOS,
        "Windows" | "windows" => PORT_CMD_WINDOWS,
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => PORT_CMD_FREEBSD,
        _ => PORT_CMD_LINUX,
    };

    format!("{metrics}; {port_cmd}; echo '===END==='\n")
}

pub fn shell_init_command(os_type: &str) -> &'static str {
    match os_type {
        "Windows" | "windows" => "set PROMPT=\r\n",
        _ => "export PS1=''; export PS2=''; stty -echo 2>/dev/null; export LANG=C\n",
    }
}

fn running_snapshot() -> ConnectionProfilerSnapshot {
    ConnectionProfilerSnapshot {
        metrics: None,
        history: Vec::with_capacity(RESOURCE_HISTORY_CAPACITY),
        state: ProfilerState::Running,
    }
}

fn spawn_profiler_thread<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    let _ = std::thread::Builder::new()
        .name("oxideterm-connection-profiler".to_string())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(future);
        });
}

async fn sample_loop(
    registry: ProfilerRegistry,
    connection_id: String,
    sampler: Arc<dyn ResourceSampler>,
    os_type: String,
    update_tx: Option<mpsc::UnboundedSender<ProfilerUpdate>>,
    mut stop_rx: oneshot::Receiver<()>,
) {
    let mut shell = match sampler
        .open_shell(shell_init_command(&os_type), RESOURCE_CHANNEL_OPEN_TIMEOUT)
        .await
    {
        Ok(shell) => shell,
        Err(_) => {
            registry.mark_degraded(&connection_id);
            record_and_emit(
                &registry,
                &update_tx,
                connection_id,
                ResourceMetrics::empty(now_ms(), MetricsSource::RttOnly),
            );
            return;
        }
    };

    let command = build_sample_command(&os_type);
    let mut previous_sample: Option<PreviousResourceSample> = None;
    let mut consecutive_failures = 0_u32;
    let mut interval = tokio::time::interval(RESOURCE_SAMPLE_INTERVAL);
    interval.tick().await;

    loop {
        tokio::select! {
            _ = &mut stop_rx => {
                let _ = shell.close().await;
                break;
            }
            _ = interval.tick() => {
                if consecutive_failures >= RESOURCE_MAX_CONSECUTIVE_FAILURES {
                    registry.mark_degraded(&connection_id);
                    record_and_emit(
                        &registry,
                        &update_tx,
                        connection_id.clone(),
                        ResourceMetrics::empty(now_ms(), MetricsSource::RttOnly),
                    );
                    continue;
                }

                match shell
                    .sample_until(
                        &command,
                        RESOURCE_END_MARKER,
                        RESOURCE_SAMPLE_TIMEOUT,
                        RESOURCE_MAX_OUTPUT_SIZE,
                    )
                    .await
                {
                    Ok(output) => {
                        consecutive_failures = 0;
                        let metrics =
                            parse_resource_metrics(&output, previous_sample.as_ref(), now_ms());
                        previous_sample = parse_cpu_snapshot(&output).map(|cpu| {
                            PreviousResourceSample {
                                cpu,
                                net: parse_net_snapshot(&output).unwrap_or_default(),
                                timestamp_ms: metrics.timestamp_ms,
                            }
                        });
                        record_and_emit(&registry, &update_tx, connection_id.clone(), metrics);
                    }
                    Err(_) => {
                        consecutive_failures = consecutive_failures.saturating_add(1);
                    }
                }
            }
        }
    }
}

fn record_and_emit(
    registry: &ProfilerRegistry,
    update_tx: &Option<mpsc::UnboundedSender<ProfilerUpdate>>,
    connection_id: String,
    metrics: ResourceMetrics,
) {
    let update = ProfilerUpdate {
        connection_id,
        metrics,
    };
    registry.record_metrics(update.clone());
    if let Some(update_tx) = update_tx {
        let _ = update_tx.send(update);
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetricsSource;

    #[test]
    fn start_is_idempotent_while_running() {
        let registry = ProfilerRegistry::new();

        assert!(registry.start("conn-1"));
        assert!(!registry.start("conn-1"));
        assert_eq!(registry.state("conn-1"), Some(ProfilerState::Running));
    }

    #[test]
    fn degraded_profiler_respawns_with_empty_history() {
        let registry = ProfilerRegistry::new();
        registry.start("conn-1");
        registry.record_metrics(ProfilerUpdate {
            connection_id: "conn-1".into(),
            metrics: ResourceMetrics::empty(1, MetricsSource::Full),
        });
        registry.mark_degraded("conn-1");

        assert!(registry.start("conn-1"));
        assert_eq!(registry.state("conn-1"), Some(ProfilerState::Running));
        assert!(registry.latest("conn-1").is_none());
        assert!(registry.history("conn-1").is_empty());
    }

    #[test]
    fn stop_and_history_match_tauri_empty_defaults() {
        let registry = ProfilerRegistry::new();
        registry.start("conn-1");
        registry.record_metrics(ProfilerUpdate {
            connection_id: "conn-1".into(),
            metrics: ResourceMetrics::empty(1, MetricsSource::Full),
        });

        assert!(registry.stop("conn-1"));
        assert!(!registry.stop("conn-1"));
        assert!(registry.latest("conn-1").is_none());
        assert!(registry.history("conn-1").is_empty());
        assert!(registry.connection_ids().is_empty());
    }

    #[test]
    fn records_only_existing_profiler_updates() {
        let registry = ProfilerRegistry::new();

        assert!(!registry.record_metrics(ProfilerUpdate {
            connection_id: "missing".into(),
            metrics: ResourceMetrics::empty(1, MetricsSource::Full),
        }));

        registry.start("conn-1");
        assert!(registry.record_metrics(ProfilerUpdate {
            connection_id: "conn-1".into(),
            metrics: ResourceMetrics::empty(2, MetricsSource::Partial),
        }));
        assert_eq!(
            registry.latest("conn-1").map(|metrics| metrics.source),
            Some(MetricsSource::Partial)
        );
    }

    #[test]
    fn builds_tauri_sampling_commands() {
        assert!(build_sample_command("Linux").contains("===STAT==="));
        assert!(build_sample_command("Linux").contains("ss -tlnp"));
        assert!(build_sample_command("Darwin").contains("lsof -iTCP"));
        assert!(build_sample_command("Windows").contains("Get-NetTCPConnection"));
        assert!(build_sample_command("FreeBSD").contains("sockstat"));
        assert!(build_sample_command("unknown").contains("ss -tlnp"));
        assert!(build_sample_command("Linux").contains("===END==="));
    }

    #[test]
    fn shell_init_matches_tauri_platform_split() {
        assert_eq!(shell_init_command("Windows"), "set PROMPT=\r\n");
        assert!(shell_init_command("Linux").contains("stty -echo"));
    }

    #[tokio::test]
    async fn sampler_open_failure_degrades_and_emits_rtt_only() {
        let registry = ProfilerRegistry::new();
        let (tx, mut rx) = mpsc::unbounded_channel();

        assert!(
            registry.start_with_sampler("conn-1", Arc::new(FailingSampler), "Linux", Some(tx),)
        );

        let update = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("degraded update should be emitted")
            .expect("update channel should stay open");

        assert_eq!(update.connection_id, "conn-1");
        assert_eq!(update.metrics.source, MetricsSource::RttOnly);
        assert_eq!(registry.state("conn-1"), Some(ProfilerState::Degraded));
        assert_eq!(
            registry.latest("conn-1").map(|metrics| metrics.source),
            Some(MetricsSource::RttOnly)
        );
    }

    #[test]
    fn start_with_sampler_without_current_tokio_runtime_does_not_panic() {
        let registry = ProfilerRegistry::new();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            registry.start_with_sampler("conn-1", Arc::new(FailingSampler), "Linux", None)
        }));

        assert!(matches!(result, Ok(true)));
        assert!(matches!(
            registry.state("conn-1"),
            Some(ProfilerState::Running | ProfilerState::Degraded)
        ));
        registry.stop("conn-1");
    }

    struct FailingSampler;

    impl ResourceSampler for FailingSampler {
        fn open_shell<'a>(
            &'a self,
            _init_command: &'a str,
            _timeout: Duration,
        ) -> ResourceSamplerFuture<'a, Result<Box<dyn ResourceSampleShell>, String>> {
            Box::pin(async { Err("open failed".to_string()) })
        }
    }
}
