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
    ResourceSystemInfo, docker_sample_command, parse_resource_metrics,
    previous_sample_from_metrics, push_history,
};

pub const RESOURCE_SAMPLE_INTERVAL: Duration = Duration::from_secs(10);
pub const RESOURCE_SAMPLE_TIMEOUT: Duration = Duration::from_secs(5);
pub const RESOURCE_CHANNEL_OPEN_TIMEOUT: Duration = Duration::from_secs(10);
// Host Tools samples include process and Docker tables. Keep enough room for
// normal inventories so SSH capture truncation does not look like a parser failure.
pub const RESOURCE_MAX_OUTPUT_SIZE: usize = 256 * 1024;
pub const RESOURCE_MAX_CONSECUTIVE_FAILURES: u32 = 3;
pub const RESOURCE_END_MARKER: &str = "===END===";

const SYSTEM_INFO_COMMAND_LINUX: &str = concat!(
    "echo '===SYSTEM_INFO==='; ",
    "if [ -r /etc/os-release ]; then ",
    "awk -F= '$1==\"NAME\"{name=substr($0,index($0,\"=\")+1)} $1==\"VERSION\"{version=substr($0,index($0,\"=\")+1)} $1==\"VERSION_ID\"{version_id=substr($0,index($0,\"=\")+1)} END{gsub(/^\"|\"$/, \"\", name);gsub(/^\"|\"$/, \"\", version);gsub(/^\"|\"$/, \"\", version_id);if(name==\"\")name=\"Linux\";if(version==\"\")version=version_id;printf \"system_name\\t%s\\nsystem_version\\t%s\\n\",name,version}' /etc/os-release 2>/dev/null; ",
    "else printf 'system_name\\t%s\\nsystem_version\\t%s\\n' \"$(uname -s 2>/dev/null)\" \"$(uname -r 2>/dev/null)\"; fi; ",
    "printf 'architecture\\t%s\\n' \"$(uname -m 2>/dev/null)\"; ",
    "boot_time=$(awk '$1==\"btime\"{print $2;exit}' /proc/stat 2>/dev/null); ",
    "case \"$boot_time\" in ''|*[!0-9]*) ;; *) printf 'boot_time_ms\\t%s000\\n' \"$boot_time\" ;; esac; ",
    "uptime_seconds=$(awk '{printf \"%.0f\",$1}' /proc/uptime 2>/dev/null); ",
    "case \"$uptime_seconds\" in ''|*[!0-9]*) ;; *) printf 'uptime_seconds\\t%s\\n' \"$uptime_seconds\" ;; esac"
);
const SYSTEM_INFO_COMMAND_MACOS: &str = concat!(
    "echo '===SYSTEM_INFO==='; ",
    "printf 'system_name\\t%s\\n' \"$(sw_vers -productName 2>/dev/null || uname -s 2>/dev/null)\"; ",
    "printf 'system_version\\t%s\\n' \"$(sw_vers -productVersion 2>/dev/null || uname -r 2>/dev/null)\"; ",
    "printf 'architecture\\t%s\\n' \"$(uname -m 2>/dev/null)\"; ",
    "boot_time=$(sysctl -n kern.boottime 2>/dev/null | awk '{for(i=1;i<=NF;i++)if($i==\"sec\"){v=$(i+2);gsub(/[^0-9]/,\"\",v);print v;exit}}'); ",
    "case \"$boot_time\" in ''|*[!0-9]*) ;; *) printf 'boot_time_ms\\t%s000\\n' \"$boot_time\"; now=$(date +%s 2>/dev/null); if [ -n \"$now\" ]; then printf 'uptime_seconds\\t%s\\n' \"$((now-boot_time))\"; fi ;; esac"
);
const SYSTEM_INFO_COMMAND_UNIX: &str = concat!(
    "echo '===SYSTEM_INFO==='; ",
    "printf 'system_name\\t%s\\n' \"$(uname -s 2>/dev/null)\"; ",
    "printf 'system_version\\t%s\\n' \"$(uname -r 2>/dev/null)\"; ",
    "printf 'architecture\\t%s\\n' \"$(uname -m 2>/dev/null)\"; ",
    "boot_time=$(sysctl -n kern.boottime 2>/dev/null | awk '{for(i=1;i<=NF;i++)if($i==\"sec\"){v=$(i+2);gsub(/[^0-9]/,\"\",v);print v;exit}}'); ",
    "case \"$boot_time\" in ''|*[!0-9]*) ;; *) printf 'boot_time_ms\\t%s000\\n' \"$boot_time\"; now=$(date +%s 2>/dev/null); if [ -n \"$now\" ]; then printf 'uptime_seconds\\t%s\\n' \"$((now-boot_time))\"; fi ;; esac"
);

const METRICS_COMMAND_LINUX_SYSTEM: &str = concat!(
    "echo '===STAT==='; grep -E '^cpu[0-9]* ' /proc/stat 2>/dev/null; ",
    "echo '===MEMINFO==='; grep -E '^(MemTotal|MemAvailable|MemFree|Buffers|Cached|SReclaimable|SwapTotal|SwapFree):' /proc/meminfo 2>/dev/null; ",
    "echo '===LOADAVG==='; cat /proc/loadavg 2>/dev/null; ",
    "echo '===NETDEV==='; cat /proc/net/dev 2>/dev/null; ",
    "echo '===NPROC==='; (nproc 2>/dev/null || grep -c '^processor' /proc/cpuinfo 2>/dev/null || true); ",
    "echo '===DISKS==='; df -P -k 2>/dev/null | awk 'NR>1 && $1 ~ /^\\/dev/ {p=$5; gsub(/%/,\"\",p); printf \"%s\\t%d\\t%d\\t%s\\n\", $6, $3*1024, $2*1024, p}'"
);
// Keep process sampling on short, machine-readable `ps` output. A previous
// `/proc` walker could emit a non-empty but unusable table, preventing fallback
// and leaving the Host Tools process page empty on otherwise healthy Linux hosts.
const METRICS_COMMAND_LINUX_PROCESSES: &str = concat!(
    "echo '===TOPPROCS==='; ",
    "mem_total=$(awk '/^MemTotal:/{print $2; exit}' /proc/meminfo 2>/dev/null); ",
    "emit_full_ps_rows() { awk 'NR<=200 && $1 ~ /^[0-9]+$/ {pid=$1;ppid=$2;user=$3;stat=$4;cpu=$5;mem=$6;rss=$7;vsz=$8;etime=$9;comm=$10;cmd=$0;sub(\"^([[:space:]]*[^[:space:]]+){10}[[:space:]]*\", \"\", cmd);gsub(/\\t/,\" \",cmd);if(cmd==\"\")cmd=comm;if(length(cmd)>240)cmd=substr(cmd,1,240);printf \"%s\\t%s\\t%s\\t%s\\t%.1f\\t%.1f\\t%s\\t%s\\t%s\\t%s\\t%s\\n\",pid,ppid,user,stat,cpu,mem,rss,vsz,etime,comm,cmd}'; }; ",
    "if ps ww -eo pid=,ppid=,user=,stat=,pcpu=,pmem=,rss=,vsz=,etime=,comm=,args= --sort=-pmem >/dev/null 2>&1; then ",
    "ps ww -eo pid=,ppid=,user=,stat=,pcpu=,pmem=,rss=,vsz=,etime=,comm=,args= --sort=-pmem 2>/dev/null | emit_full_ps_rows; ",
    "elif ps ww -eo pid=,ppid=,user=,stat=,pcpu=,pmem=,rss=,vsz=,etime=,comm=,args= >/dev/null 2>&1; then ",
    "ps ww -eo pid=,ppid=,user=,stat=,pcpu=,pmem=,rss=,vsz=,etime=,comm=,args= 2>/dev/null | sort -k6 -rn | emit_full_ps_rows; ",
    "else ",
    "ps -o pid,vsz,comm 2>/dev/null | awk -v total=\"$mem_total\" 'NR>1 && NR<=201 && $1 ~ /^[0-9]+$/ {vsz=$2;mem=(total>0?vsz*100/total:0);gsub(/\\t/,\" \",$3);printf \"%s\\t\\t\\t\\t\\t%.1f\\t\\t%s\\t\\t%s\\t%s\\n\",$1,mem,vsz,$3,$3}'; ",
    "fi"
);
const METRICS_COMMAND_LINUX_GPU: &str = concat!(
    "echo '===GPUS==='; ",
    "if command -v nvidia-smi >/dev/null 2>&1; then ",
    "nvidia-smi --query-gpu=index,name,utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits 2>/dev/null; ",
    "else ",
    "idx=0; ",
    "for dev in /sys/class/drm/card*/device; do ",
    "[ -d \"$dev\" ] || continue; ",
    "vendor=$(cat \"$dev/vendor\" 2>/dev/null); ",
    "case \"$vendor\" in 0x1002|0x8086) ;; *) continue ;; esac; ",
    "util=$(cat \"$dev/gpu_busy_percent\" 2>/dev/null || true); ",
    "total=$(cat \"$dev/mem_info_vram_total\" 2>/dev/null || true); ",
    "used=$(cat \"$dev/mem_info_vram_used\" 2>/dev/null || true); ",
    "[ -n \"$util$used$total\" ] || continue; ",
    "if [ -r \"$dev/product_name\" ]; then name=$(cat \"$dev/product_name\" 2>/dev/null); elif [ \"$vendor\" = \"0x8086\" ]; then name='Intel GPU'; else name='AMD GPU'; fi; ",
    "used_mib=$(awk -v v=\"$used\" 'BEGIN{if(v~/^[0-9]+$/)printf \"%.0f\",v/1048576; else printf \"\"}'); ",
    "total_mib=$(awk -v v=\"$total\" 'BEGIN{if(v~/^[0-9]+$/)printf \"%.0f\",v/1048576; else printf \"\"}'); ",
    "printf \"%s,%s,%s,%s,%s\\n\" \"$idx\" \"$name\" \"$util\" \"$used_mib\" \"$total_mib\"; ",
    "idx=$((idx+1)); ",
    "done; ",
    "if [ \"$idx\" -eq 0 ] && command -v rocm-smi >/dev/null 2>&1; then ",
    "rocm-smi --showuse --showmemuse --showproductname --csv 2>/dev/null | awk -F, 'NR>1 {gsub(/^ +| +$/, \"\", $0); idx=$1; name=$2; util=$3; mem=$4; gsub(/[^0-9.]/, \"\", idx); gsub(/^ +| +$/, \"\", name); gsub(/[^0-9.]/, \"\", util); gsub(/[^0-9.]/, \"\", mem); if(idx!=\"\") printf \"%s,%s,%s,,\\n\", idx, name, util}'; ",
    "fi; ",
    "fi; ",
    "echo '===GPUS_INTEL_TOP==='; ",
    "if command -v intel_gpu_top >/dev/null 2>&1 && command -v timeout >/dev/null 2>&1; then ",
    "timeout 3 intel_gpu_top -J -s 1000 -n 2 -o - 2>/dev/null || true; ",
    "fi"
);
const METRICS_COMMAND_MACOS_SYSTEM: &str = "echo '===CPU_DIRECT==='; cpuline=$(top -l 1 -s 0 -n 0 2>/dev/null | grep 'CPU usage:' | head -1); echo \"$cpuline\" | awk '{for(i=1;i<=NF;i++){if($(i+1)~/^idle/){v=$i;gsub(/%/,\"\",v);printf \"%.1f\\n\",100-v}}}'; echo '===MEMINFO==='; pagesize=$(sysctl -n hw.pagesize 2>/dev/null || echo 4096); memtotal=$(sysctl -n hw.memsize 2>/dev/null | awk '{printf \"%d\",$1/1024}'); vm_stat 2>/dev/null | awk -v ps=\"$pagesize\" -v total=\"$memtotal\" 'BEGIN{free=0;spec=0;inactive=0;purgeable=0} /^Pages free:/{gsub(/[^0-9]/,\"\",$NF);free=$NF} /^Pages speculative:/{gsub(/[^0-9]/,\"\",$NF);spec=$NF} /^Pages inactive:/{gsub(/[^0-9]/,\"\",$NF);inactive=$NF} /^Pages purgeable:/{gsub(/[^0-9]/,\"\",$NF);purgeable=$NF} END{avail=int((free+spec+inactive+purgeable)*ps/1024); printf \"MemTotal: %d kB\\nMemAvailable: %d kB\\n\",total,avail}'; sysctl vm.swapusage 2>/dev/null | awk '{for(i=1;i<=NF;i++){if($i==\"total\"&&$(i+1)==\"=\"){v=$(i+2);m=1024;if(v~/G/)m=1048576;gsub(/[MmGg]/,\"\",v);total=v*m} if($i==\"used\"&&$(i+1)==\"=\"){v=$(i+2);m=1024;if(v~/G/)m=1048576;gsub(/[MmGg]/,\"\",v);used=v*m}} printf \"SwapTotal: %.0f kB\\nSwapFree: %.0f kB\\n\",total,total-used}'; echo '===LOADAVG==='; sysctl -n vm.loadavg 2>/dev/null | tr -d '{}'; echo '===NETDEV==='; netstat -ib 2>/dev/null | awk '/^[a-z]/&&$3~/Link/&&$1!~/^lo/{if($4~/:/){rx=$7;tx=$10}else{rx=$6;tx=$9};if((rx+0)>0){gsub(/[\\*]/,\"\",$1);printf \"%s: %s 0 0 0 0 0 0 0 %s\\n\",$1,rx,tx}}'; echo '===NPROC==='; sysctl -n hw.logicalcpu 2>/dev/null; echo '===DISKS==='; df -P -k 2>/dev/null | awk 'NR>1 && $1 ~ /^\\/dev/ && ($6==\"/\" || $6 ~ /^\\/Volumes\\//) {p=$5; gsub(/%/,\"\",p); printf \"%s\\t%d\\t%d\\t%s\\n\", $6, $3*1024, $2*1024, p}'";
const METRICS_COMMAND_MACOS_PROCESSES: &str = "echo '===TOPPROCS==='; ps axww -o pid=,ppid=,user=,stat=,pcpu=,pmem=,rss=,vsz=,etime=,comm=,command= 2>/dev/null | sort -k6 -rn | awk 'NR<=200 {pid=$1;ppid=$2;user=$3;stat=$4;cpu=$5;mem=$6;rss=$7;vsz=$8;etime=$9;comm=$10;$1=$2=$3=$4=$5=$6=$7=$8=$9=$10=\"\";sub(/^ +/,\"\");gsub(/\\t/,\" \");printf \"%s\\t%s\\t%s\\t%s\\t%.1f\\t%.1f\\t%s\\t%s\\t%s\\t%s\\t%s\\n\",pid,ppid,user,stat,cpu,mem,rss,vsz,etime,comm,$0}'";
const METRICS_COMMAND_UNSUPPORTED: &str =
    "echo '===UNSUPPORTED==='; uname -s 2>/dev/null || echo unknown";

/// Selects only the recurring probes the user has enabled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceSamplingConfig {
    pub system: bool,
    pub gpu: bool,
    pub processes: bool,
    pub docker: bool,
}

impl ResourceSamplingConfig {
    pub fn is_empty(self) -> bool {
        !self.system && !self.gpu && !self.processes && !self.docker
    }
}

impl Default for ResourceSamplingConfig {
    fn default() -> Self {
        Self {
            system: true,
            gpu: true,
            processes: true,
            docker: true,
        }
    }
}

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
    config: ResourceSamplingConfig,
    stop_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

#[derive(Clone)]
struct CachedSystemInfo {
    value: ResourceSystemInfo,
    sampled_at_ms: u64,
}

impl CachedSystemInfo {
    fn snapshot_at(&self, timestamp_ms: u64) -> ResourceSystemInfo {
        let mut value = self.value.clone();
        if let Some(sampled_uptime) = value.uptime_seconds {
            let elapsed_seconds = timestamp_ms.saturating_sub(self.sampled_at_ms) / 1_000;
            value.uptime_seconds = Some(sampled_uptime.saturating_add(elapsed_seconds));
        }
        value
    }
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
                config: ResourceSamplingConfig::default(),
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
        self.start_with_sampler_on_handle(
            connection_id,
            sampler,
            os_type,
            ResourceSamplingConfig::default(),
            update_tx,
            spawn_handle,
        )
    }

    pub fn start_with_sampler_on(
        &self,
        connection_id: impl Into<String>,
        sampler: Arc<dyn ResourceSampler>,
        os_type: impl Into<String>,
        update_tx: Option<mpsc::UnboundedSender<ProfilerUpdate>>,
        handle: Handle,
    ) -> bool {
        self.start_with_sampler_on_config(
            connection_id,
            sampler,
            os_type,
            ResourceSamplingConfig::default(),
            update_tx,
            handle,
        )
    }

    /// Restarts a running sampler when its enabled probe set changes.
    pub fn start_with_sampler_on_config(
        &self,
        connection_id: impl Into<String>,
        sampler: Arc<dyn ResourceSampler>,
        os_type: impl Into<String>,
        config: ResourceSamplingConfig,
        update_tx: Option<mpsc::UnboundedSender<ProfilerUpdate>>,
        handle: Handle,
    ) -> bool {
        self.start_with_sampler_on_handle(
            connection_id,
            sampler,
            os_type,
            config,
            update_tx,
            Some(handle),
        )
    }

    fn start_with_sampler_on_handle(
        &self,
        connection_id: impl Into<String>,
        sampler: Arc<dyn ResourceSampler>,
        os_type: impl Into<String>,
        config: ResourceSamplingConfig,
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
                    .map(|entry| (entry.snapshot.state, entry.config)),
                Some((ProfilerState::Running, running_config)) if running_config == config
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
                    config,
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
                config,
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

    /// Return the live state without cloning history for high-frequency UI renders.
    pub fn current(&self, connection_id: &str) -> Option<(Option<ResourceMetrics>, ProfilerState)> {
        lock(&self.profilers)
            .get(connection_id)
            .map(|entry| (entry.snapshot.metrics.clone(), entry.snapshot.state))
    }

    pub fn connection_ids(&self) -> Vec<String> {
        lock(&self.profilers).keys().cloned().collect()
    }
}

pub fn build_sample_command(os_type: &str) -> String {
    build_sample_command_for(os_type, ResourceSamplingConfig::default())
}

pub fn build_sample_command_for(os_type: &str, config: ResourceSamplingConfig) -> String {
    build_sample_command_with_system_info(os_type, config, true)
}

fn build_live_sample_command(os_type: &str, config: ResourceSamplingConfig) -> String {
    build_sample_command_with_system_info(os_type, config, false)
}

fn build_sample_command_with_system_info(
    os_type: &str,
    config: ResourceSamplingConfig,
    include_system_info: bool,
) -> String {
    if matches!(os_type, "Windows" | "windows") {
        return build_windows_sample_command(config, include_system_info);
    }

    let mut commands = Vec::new();
    if include_system_info && config.system {
        let system_info = match os_type {
            "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => {
                SYSTEM_INFO_COMMAND_LINUX
            }
            "macOS" | "macos" | "Darwin" => SYSTEM_INFO_COMMAND_MACOS,
            "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => SYSTEM_INFO_COMMAND_UNIX,
            _ => SYSTEM_INFO_COMMAND_UNIX,
        };
        commands.push(system_info.to_string());
    }

    if config.system {
        let metrics = match os_type {
            "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => {
                METRICS_COMMAND_LINUX_SYSTEM
            }
            "macOS" | "macos" | "Darwin" => METRICS_COMMAND_MACOS_SYSTEM,
            "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => METRICS_COMMAND_UNSUPPORTED,
            _ => METRICS_COMMAND_UNSUPPORTED,
        };
        commands.push(metrics.to_string());
    }
    if config.gpu
        && matches!(
            os_type,
            "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin"
        )
    {
        commands.push(METRICS_COMMAND_LINUX_GPU.to_string());
    }
    if config.processes {
        let process_metrics = match os_type {
            "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => {
                Some(METRICS_COMMAND_LINUX_PROCESSES)
            }
            "macOS" | "macos" | "Darwin" => Some(METRICS_COMMAND_MACOS_PROCESSES),
            _ => None,
        };
        if let Some(process_metrics) = process_metrics {
            commands.push(process_metrics.to_string());
        }
    }
    if config.docker {
        commands.push(docker_sample_command(os_type).to_string());
    }
    commands.push("echo '===END==='".to_string());
    format!("{}\n", commands.join("; "))
}

fn build_windows_sample_command(
    config: ResourceSamplingConfig,
    include_system_info: bool,
) -> String {
    let mut script = String::from("$ErrorActionPreference='SilentlyContinue';");
    if config.system || config.processes {
        script.push_str("$os=Get-CimInstance Win32_OperatingSystem;");
    }
    if include_system_info && config.system {
        script.push_str(concat!(
            "Write-Output '===SYSTEM_INFO===';",
            "if($os){",
            "Write-Output ('system_name'+[char]9+$os.Caption);",
            "Write-Output ('system_version'+[char]9+($os.Version+' (Build '+$os.BuildNumber+')'));",
            "Write-Output ('architecture'+[char]9+$os.OSArchitecture);",
            "$boot=[DateTimeOffset]$os.LastBootUpTime;",
            "Write-Output ('boot_time_ms'+[char]9+$boot.ToUnixTimeMilliseconds());",
            "$uptime=[UInt64][Math]::Max(0,[Math]::Floor(((Get-Date)-$os.LastBootUpTime).TotalSeconds));",
            "Write-Output ('uptime_seconds'+[char]9+$uptime);",
            "};"
        ));
    }
    if config.system {
        script.push_str(concat!(
            "Write-Output '===CPU_DIRECT===';",
            "$cpu=(Get-CimInstance Win32_Processor|Measure-Object -Property LoadPercentage -Average).Average;",
            "if($cpu -ne $null){[Math]::Round($cpu,1)};",
            "Write-Output '===MEMINFO===';",
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
            "};"
        ));
    }
    if config.gpu {
        script.push_str(concat!(
            "Write-Output '===GPUS===';",
            "$gpuControllers=@(Get-CimInstance Win32_VideoController);",
            "$gpuUtil=@{};$gpuMem=@{};",
            "try{",
            "$samples=(Get-Counter '\\GPU Engine(*)\\Utilization Percentage').CounterSamples;",
            "foreach($sample in $samples){",
            "$instance=$sample.InstanceName;",
            "$phys=0;",
            "if($instance -match 'phys_([0-9]+)'){$phys=[int]$matches[1]};",
            "$gpuUtil[$phys]=[double]($gpuUtil[$phys])+[double]$sample.CookedValue",
            "}",
            "}catch{};",
            "try{",
            "$memSamples=(Get-Counter '\\GPU Adapter Memory(*)\\Dedicated Usage').CounterSamples;",
            "foreach($sample in $memSamples){",
            "$instance=$sample.InstanceName;",
            "$phys=0;",
            "if($instance -match 'phys_([0-9]+)'){$phys=[int]$matches[1]};",
            "$gpuMem[$phys]=[double]($gpuMem[$phys])+[double]$sample.CookedValue",
            "}",
            "}catch{};",
            "for($i=0;$i -lt $gpuControllers.Count;$i++){",
            "$gpu=$gpuControllers[$i];",
            "$name=($gpu.Name -replace ',', ' ');",
            "$total=if($gpu.AdapterRAM){[Math]::Round(([double]$gpu.AdapterRAM)/1MB)}else{''};",
            "$used=if($gpuMem.ContainsKey($i)){[Math]::Round(([double]$gpuMem[$i])/1MB)}else{''};",
            "$util=if($gpuUtil.ContainsKey($i)){[Math]::Min(100,[Math]::Round([double]$gpuUtil[$i],1))}else{''};",
            "Write-Output ($i+','+$name+','+$util+','+$used+','+$total)",
            "};"
        ));
    }
    if config.processes {
        script.push_str(concat!(
            "Write-Output '===TOPPROCS===';",
            "$memTotal=if($os){[double]$os.TotalVisibleMemorySize*1024}else{0};",
            "Get-Process|Sort-Object WorkingSet64 -Descending|Select-Object -First 200|ForEach-Object{",
            "$pct=if($memTotal -gt 0){[Math]::Round(($_.WorkingSet64*100)/$memTotal,1)}else{0};",
            "$cpu=if($_.CPU -ne $null){[Math]::Round($_.CPU,1)}else{0};",
            "$rss=[UInt64]$_.WorkingSet64;$vsz=[UInt64]$_.VirtualMemorySize64;",
            "$elapsed=if($_.StartTime){((Get-Date)-$_.StartTime).ToString()}else{''};",
            "Write-Output ($_.Id+[char]9+''+[char]9+''+[char]9+''+[char]9+$cpu+[char]9+$pct+[char]9+[Math]::Round($rss/1024)+[char]9+[Math]::Round($vsz/1024)+[char]9+$elapsed+[char]9+$_.ProcessName+[char]9+$_.Path)",
            "};"
        ));
    }
    if config.docker {
        script.push_str(&docker_sample_command("Windows"));
    }
    script.push_str("Write-Output '===END===';");
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
    config: ResourceSamplingConfig,
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

    let initial_command = build_sample_command_for(&os_type, config);
    let live_command = build_live_sample_command(&os_type, config);
    let mut system_info_sampled = false;
    let mut cached_system_info: Option<CachedSystemInfo> = None;
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
                    let timestamp_ms = now_ms();
                    record_and_emit(
                        &registry,
                        &update_tx,
                        connection_id.clone(),
                        empty_metrics_with_cached_system_info(
                            timestamp_ms,
                            MetricsSource::Unsupported,
                            cached_system_info.as_ref(),
                        ),
                        );
                    let _ = shell.close().await;
                    break;
                    }

                let command = if system_info_sampled {
                    &live_command
                } else {
                    &initial_command
                };
                match shell
                    .sample_until(
                        command,
                        RESOURCE_END_MARKER,
                        RESOURCE_SAMPLE_TIMEOUT,
                        RESOURCE_MAX_OUTPUT_SIZE,
                    )
                    .await
                {
                    Ok(output) => {
                        let timestamp_ms = now_ms();
                        let mut metrics =
                            parse_resource_metrics(&output, previous_sample.as_ref(), timestamp_ms);
                        if !system_info_sampled {
                            system_info_sampled = true;
                            if let Some(system_info) = metrics.system_info.clone() {
                                cached_system_info = Some(CachedSystemInfo {
                                    value: system_info,
                                    sampled_at_ms: timestamp_ms,
                                });
                            }
                        } else if metrics.system_info.is_none() {
                            metrics.system_info = cached_system_info
                                .as_ref()
                                .map(|cached| cached.snapshot_at(timestamp_ms));
                        }
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
                            let timestamp_ms = now_ms();
                            record_and_emit(
                                &registry,
                                &update_tx,
                                connection_id.clone(),
                                empty_metrics_with_cached_system_info(
                                    timestamp_ms,
                                    MetricsSource::Unsupported,
                                    cached_system_info.as_ref(),
                                ),
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
                            let timestamp_ms = now_ms();
                            record_and_emit(
                                &registry,
                                &update_tx,
                                connection_id.clone(),
                                empty_metrics_with_cached_system_info(
                                    timestamp_ms,
                                    MetricsSource::Unsupported,
                                    cached_system_info.as_ref(),
                                ),
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
                            empty_metrics_with_cached_system_info(
                                now_ms(),
                                MetricsSource::Failed,
                                cached_system_info.as_ref(),
                            ),
                        );
                    }
                }
            }
        }
    }
}

fn empty_metrics_with_cached_system_info(
    timestamp_ms: u64,
    source: MetricsSource,
    cached_system_info: Option<&CachedSystemInfo>,
) -> ResourceMetrics {
    let mut metrics = ResourceMetrics::empty(timestamp_ms, source);
    // Preserve stable identity metadata when one live sample fails or becomes unsupported.
    metrics.system_info = cached_system_info.map(|cached| cached.snapshot_at(timestamp_ms));
    metrics
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
    fn current_omits_history_for_lightweight_render_paths() {
        let registry = ProfilerRegistry::new();
        registry.start("conn-1");
        for timestamp_ms in 0..3 {
            registry.record_metrics(ProfilerUpdate {
                connection_id: "conn-1".into(),
                metrics: ResourceMetrics::empty(timestamp_ms, MetricsSource::Full),
            });
        }

        assert_eq!(
            registry.current("conn-1"),
            Some((
                Some(ResourceMetrics::empty(2, MetricsSource::Full)),
                ProfilerState::Running,
            ))
        );
        assert_eq!(registry.history("conn-1").len(), 3);
    }

    #[test]
    fn sampling_command_includes_only_enabled_recurring_probes() {
        let processes_only = build_sample_command_for(
            "Linux",
            ResourceSamplingConfig {
                system: false,
                gpu: false,
                processes: true,
                docker: false,
            },
        );

        assert!(processes_only.contains("===TOPPROCS==="));
        assert!(!processes_only.contains("===STAT==="));
        assert!(!processes_only.contains("===GPUS==="));
        assert!(!processes_only.contains("===DOCKER==="));
        assert!(!processes_only.contains("===PORTS==="));

        let docker_only = build_sample_command_for(
            "Windows",
            ResourceSamplingConfig {
                system: false,
                gpu: false,
                processes: false,
                docker: true,
            },
        );
        assert!(docker_only.contains("===DOCKER==="));
        assert!(!docker_only.contains("Win32_OperatingSystem"));
        assert!(!docker_only.contains("===TOPPROCS==="));
        assert!(!docker_only.contains("===PORTS==="));
    }

    #[cfg(unix)]
    #[test]
    fn unix_sampling_commands_have_valid_posix_shell_syntax() {
        for os_type in ["Linux", "Darwin", "FreeBSD"] {
            let status = std::process::Command::new("sh")
                .args(["-n", "-c", &build_sample_command(os_type)])
                .status()
                .expect("run POSIX shell syntax check");
            assert!(status.success(), "{os_type} sample command should parse");
        }
    }

    #[test]
    fn cached_system_information_advances_uptime_and_survives_empty_samples() {
        let cached = CachedSystemInfo {
            value: ResourceSystemInfo {
                system_name: Some("Ubuntu".to_string()),
                system_version: Some("24.04.3 LTS".to_string()),
                architecture: Some("x86_64".to_string()),
                boot_time_ms: Some(1_000),
                uptime_seconds: Some(60),
            },
            sampled_at_ms: 10_000,
        };

        let metrics =
            empty_metrics_with_cached_system_info(15_500, MetricsSource::Failed, Some(&cached));
        let system_info = metrics.system_info.expect("cached system info");

        assert_eq!(system_info.system_name.as_deref(), Some("Ubuntu"));
        assert_eq!(system_info.uptime_seconds, Some(65));
        assert_eq!(metrics.source, MetricsSource::Failed);
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
