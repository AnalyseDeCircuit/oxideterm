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
    parse_resource_metrics, previous_sample_from_metrics, push_history,
};

pub const RESOURCE_SAMPLE_INTERVAL: Duration = Duration::from_secs(10);
pub const RESOURCE_SAMPLE_TIMEOUT: Duration = Duration::from_secs(5);
pub const RESOURCE_CHANNEL_OPEN_TIMEOUT: Duration = Duration::from_secs(10);
pub const RESOURCE_MAX_OUTPUT_SIZE: usize = 65_536;
pub const RESOURCE_MAX_CONSECUTIVE_FAILURES: u32 = 3;
pub const RESOURCE_END_MARKER: &str = "===END===";

const METRICS_COMMAND_LINUX: &str = "echo '===STAT==='; grep -E '^cpu[0-9]* ' /proc/stat 2>/dev/null; echo '===MEMINFO==='; grep -E '^(MemTotal|MemAvailable|MemFree|Buffers|Cached|SReclaimable|SwapTotal|SwapFree):' /proc/meminfo 2>/dev/null; echo '===LOADAVG==='; cat /proc/loadavg 2>/dev/null; echo '===NETDEV==='; cat /proc/net/dev 2>/dev/null; echo '===NPROC==='; (nproc 2>/dev/null || grep -c '^processor' /proc/cpuinfo 2>/dev/null || true); echo '===DISKS==='; df -P -k 2>/dev/null | awk 'NR>1 && $1 ~ /^\\/dev/ {p=$5; gsub(/%/,\"\",p); printf \"%s\\t%d\\t%d\\t%s\\n\", $6, $3*1024, $2*1024, p}'; echo '===TOPPROCS==='; ((ps -eo pid=,%mem=,comm= --sort=-%mem 2>/dev/null | head -10 | awk '{gsub(/\\t/,\" \",$3); printf \"%s\\t%.1f\\t%s\\n\",$1,$2,$3}') || (ps -o pid,vsz,comm 2>/dev/null | awk 'NR>1 {print $2,$1,$3}' | sort -rn | head -10 | awk -v total=$(awk '/^MemTotal:/{print $2}' /proc/meminfo 2>/dev/null) '{pct=(total>0?$1*100/total:0); printf \"%s\\t%.1f\\t%s\\n\",$2,pct,$3}'))";
const METRICS_COMMAND_MACOS: &str = "echo '===CPU_DIRECT==='; cpuline=$(top -l 1 -s 0 -n 0 2>/dev/null | grep 'CPU usage:' | head -1); echo \"$cpuline\" | awk '{for(i=1;i<=NF;i++){if($(i+1)~/^idle/){v=$i;gsub(/%/,\"\",v);printf \"%.1f\\n\",100-v}}}'; echo '===MEMINFO==='; pagesize=$(sysctl -n hw.pagesize 2>/dev/null || echo 4096); memtotal=$(sysctl -n hw.memsize 2>/dev/null | awk '{printf \"%d\",$1/1024}'); vm_stat 2>/dev/null | awk -v ps=\"$pagesize\" -v total=\"$memtotal\" 'BEGIN{free=0;spec=0;inactive=0;purgeable=0} /^Pages free:/{gsub(/[^0-9]/,\"\",$NF);free=$NF} /^Pages speculative:/{gsub(/[^0-9]/,\"\",$NF);spec=$NF} /^Pages inactive:/{gsub(/[^0-9]/,\"\",$NF);inactive=$NF} /^Pages purgeable:/{gsub(/[^0-9]/,\"\",$NF);purgeable=$NF} END{avail=int((free+spec+inactive+purgeable)*ps/1024); printf \"MemTotal: %d kB\\nMemAvailable: %d kB\\n\",total,avail}'; sysctl vm.swapusage 2>/dev/null | awk '{for(i=1;i<=NF;i++){if($i==\"total\"&&$(i+1)==\"=\"){v=$(i+2);m=1024;if(v~/G/)m=1048576;gsub(/[MmGg]/,\"\",v);total=v*m} if($i==\"used\"&&$(i+1)==\"=\"){v=$(i+2);m=1024;if(v~/G/)m=1048576;gsub(/[MmGg]/,\"\",v);used=v*m}} printf \"SwapTotal: %.0f kB\\nSwapFree: %.0f kB\\n\",total,total-used}'; echo '===LOADAVG==='; sysctl -n vm.loadavg 2>/dev/null | tr -d '{}'; echo '===NETDEV==='; netstat -ib 2>/dev/null | awk '/^[a-z]/&&$3~/Link/&&$1!~/^lo/{if($4~/:/){rx=$7;tx=$10}else{rx=$6;tx=$9};if((rx+0)>0){gsub(/[\\*]/,\"\",$1);printf \"%s: %s 0 0 0 0 0 0 0 %s\\n\",$1,rx,tx}}'; echo '===NPROC==='; sysctl -n hw.logicalcpu 2>/dev/null; echo '===DISKS==='; df -P -k 2>/dev/null | awk 'NR>1 && $1 ~ /^\\/dev/ && ($6==\"/\" || $6 ~ /^\\/Volumes\\//) {p=$5; gsub(/%/,\"\",p); printf \"%s\\t%d\\t%d\\t%s\\n\", $6, $3*1024, $2*1024, p}'; echo '===TOPPROCS==='; ps -A -o pid=,%mem=,comm= 2>/dev/null | sort -k2 -rn | head -10 | awk '{printf \"%s\\t%.1f\\t%s\\n\",$1,$2,$3}'";
const METRICS_COMMAND_UNSUPPORTED: &str =
    "echo '===UNSUPPORTED==='; uname -s 2>/dev/null || echo unknown";
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
    let metrics = match os_type {
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => {
            METRICS_COMMAND_LINUX
        }
        "macOS" | "macos" | "Darwin" => METRICS_COMMAND_MACOS,
        "Windows" | "windows" => return build_windows_sample_command(),
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => METRICS_COMMAND_UNSUPPORTED,
        _ => METRICS_COMMAND_UNSUPPORTED,
    };
    let port_cmd = match os_type {
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => PORT_CMD_LINUX,
        "macOS" | "macos" | "Darwin" => PORT_CMD_MACOS,
        "Windows" | "windows" => PORT_CMD_WINDOWS,
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => PORT_CMD_FREEBSD,
        _ => PORT_CMD_LINUX,
    };

    format!("{metrics}; {port_cmd}; echo '===END==='\n")
}

fn build_windows_sample_command() -> String {
    let script = concat!(
        "$ErrorActionPreference='SilentlyContinue';",
        "Write-Output '===CPU_DIRECT===';",
        "$cpu=(Get-CimInstance Win32_Processor|Measure-Object -Property LoadPercentage -Average).Average;",
        "if($cpu -ne $null){[Math]::Round($cpu,1)};",
        "Write-Output '===MEMINFO===';",
        "$os=Get-CimInstance Win32_OperatingSystem;",
        "if($os){",
        "Write-Output ('MemTotal: '+$os.TotalVisibleMemorySize+' kB');",
        "Write-Output ('MemAvailable: '+$os.FreePhysicalMemory+' kB');",
        "$st=[UInt64]([Math]::Max(0,$os.TotalVirtualMemorySize-$os.TotalVisibleMemorySize));",
        "$sf=[UInt64]([Math]::Max(0,$os.FreeVirtualMemory-$os.FreePhysicalMemory));",
        "Write-Output ('SwapTotal: '+$st+' kB');",
        "Write-Output ('SwapFree: '+$sf+' kB');",
        "};",
        "Write-Output '===NPROC===';",
        "$cores=(Get-CimInstance Win32_Processor|Measure-Object -Property NumberOfLogicalProcessors -Sum).Sum;",
        "if($cores){$cores};",
        "Write-Output '===DISKS===';",
        "Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3'|ForEach-Object{",
        "$total=[UInt64]$_.Size;$free=[UInt64]$_.FreeSpace;$used=$total-$free;",
        "$pct=if($total -gt 0){[Math]::Round(($used*100)/$total,1)}else{0};",
        "Write-Output ($_.DeviceID+[char]9+$used+[char]9+$total+[char]9+$pct)",
        "};",
        "Write-Output '===NETDEV===';",
        "Get-NetAdapterStatistics|ForEach-Object{",
        "Write-Output ($_.Name+': '+$_.ReceivedBytes+' 0 0 0 0 0 0 0 '+$_.SentBytes)",
        "};",
        "Write-Output '===TOPPROCS===';",
        "$memTotal=if($os){[double]$os.TotalVisibleMemorySize*1024}else{0};",
        "Get-Process|Sort-Object WorkingSet64 -Descending|Select-Object -First 10|ForEach-Object{",
        "$pct=if($memTotal -gt 0){[Math]::Round(($_.WorkingSet64*100)/$memTotal,1)}else{0};",
        "Write-Output ($_.Id+[char]9+$pct+[char]9+$_.ProcessName)",
        "};",
        "Write-Output '===PORTS===';",
        "Get-NetTCPConnection -State Listen|ForEach-Object{",
        "Write-Output ($_.LocalAddress+' '+$_.LocalPort+' '+$_.OwningProcess)",
        "};",
        "Write-Output '===PORTS_END===';",
        "Write-Output '===END===';"
    );
    // OpenSSH on Windows may start cmd.exe or PowerShell; invoking PowerShell
    // explicitly keeps the sampler independent from the user's default shell.
    format!("powershell -NoProfile -ExecutionPolicy Bypass -Command \"{script}\"\r\n")
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
    let mut shell = match open_resource_sample_shell(sampler.as_ref(), &os_type).await {
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
                        ResourceMetrics::empty(now_ms(), MetricsSource::Unsupported),
                        );
                    let _ = shell.close().await;
                    break;
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
                        let metrics =
                            parse_resource_metrics(&output, previous_sample.as_ref(), now_ms());
                        if matches!(
                            metrics.source,
                            MetricsSource::RttOnly | MetricsSource::Unsupported
                        ) {
                            consecutive_failures = consecutive_failures.saturating_add(1);
                        } else {
                            consecutive_failures = 0;
                        }
                        if consecutive_failures >= RESOURCE_MAX_CONSECUTIVE_FAILURES {
                            registry.mark_degraded(&connection_id);
                            record_and_emit(
                                &registry,
                                &update_tx,
                                connection_id.clone(),
                                ResourceMetrics::empty(now_ms(), MetricsSource::Unsupported),
                            );
                            let _ = shell.close().await;
                            break;
                        }
                        previous_sample = previous_sample_from_metrics(&metrics, &output);
                        record_and_emit(&registry, &update_tx, connection_id.clone(), metrics);
                    }
                    Err(_) => {
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        if consecutive_failures >= RESOURCE_MAX_CONSECUTIVE_FAILURES {
                            registry.mark_degraded(&connection_id);
                            record_and_emit(
                                &registry,
                                &update_tx,
                                connection_id.clone(),
                                ResourceMetrics::empty(now_ms(), MetricsSource::Unsupported),
                            );
                            let _ = shell.close().await;
                            break;
                        }
                        // Tauri writes a Failed sample on each transient read
                        // failure and then tries to reopen the persistent shell
                        // once. Without that update, the native UI can look
                        // inert until the profiler finally degrades.
                        if let Ok(new_shell) =
                            open_resource_sample_shell(sampler.as_ref(), &os_type).await
                        {
                            shell = new_shell;
                        }
                        record_and_emit(
                            &registry,
                            &update_tx,
                            connection_id.clone(),
                            ResourceMetrics::empty(now_ms(), MetricsSource::Failed),
                        );
                    }
                }
            }
        }
    }
}

async fn open_resource_sample_shell(
    sampler: &dyn ResourceSampler,
    os_type: &str,
) -> Result<Box<dyn ResourceSampleShell>, String> {
    sampler
        .open_shell(shell_init_command(os_type), RESOURCE_CHANNEL_OPEN_TIMEOUT)
        .await
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
        let linux = build_sample_command("Linux");
        assert!(linux.contains("===STAT==="));
        assert!(linux.contains("===DISKS==="));
        assert!(linux.contains("===TOPPROCS==="));
        let nproc_marker = linux.find("===NPROC===").expect("nproc marker");
        let disk_marker = linux.find("===DISKS===").expect("disk marker");
        assert!(
            nproc_marker < disk_marker,
            "nproc should be sampled before disk summaries"
        );
        assert!(linux.contains("ss -tlnp"));
        assert!(build_sample_command("Darwin").contains("lsof -iTCP"));
        assert!(build_sample_command("Windows").contains("Get-NetTCPConnection"));
        let freebsd = build_sample_command("FreeBSD");
        assert!(freebsd.contains("sockstat"));
        assert!(freebsd.contains("===UNSUPPORTED==="));
        assert!(build_sample_command("unknown").contains("===UNSUPPORTED==="));
        assert!(linux.contains("===END==="));
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
