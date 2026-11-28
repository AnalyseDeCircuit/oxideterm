use gpui::{prelude::*, *};
use oxideterm_connection_monitor::*;
use oxideterm_gpui_host_tools::*;
use oxideterm_gpui_ui::text_input::{TextInputAnchorId, TextInputViewport};
use oxideterm_i18n::I18n;
use oxideterm_settings::HostToolsSettings;
use oxideterm_theme::ThemeTokens;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use zeroize::Zeroizing;

const HOST_LOG_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 560.0;
const MONITOR_BORDER_ALPHA: u32 = 0x80;
const HISTORY_CAP: usize = 30;

struct DemoHostCapture;

impl HostCapture for DemoHostCapture {
    fn run_command_capture<'a>(
        &'a self,
        command: &'a str,
        _timeout: Duration,
        _max_output_size: usize,
    ) -> HostCaptureFuture<'a, HostCommandOutput> {
        let stdout = demo_command_output(command);
        Box::pin(async move {
            Ok(HostCommandOutput {
                stdout,
                stderr: String::new(),
                exit_code: Some(0),
                truncated: false,
            })
        })
    }

    fn run_secret_command_capture_with_stdin<'a>(
        &'a self,
        command: &'a str,
        _stdin: Zeroizing<Vec<u8>>,
        _timeout: Duration,
        _max_output_size: usize,
    ) -> HostCaptureFuture<'a, HostSecretCommandOutput> {
        let stdout = demo_command_output(command);
        Box::pin(async move {
            Ok(HostSecretCommandOutput {
                stdout: Zeroizing::new(stdout.into_bytes()),
                stderr: Zeroizing::new(Vec::new()),
                exit_code: Some(0),
                truncated: false,
            })
        })
    }
}

fn demo_command_output(command: &str) -> String {
    if command.contains("===HOST_LOGS===") {
        concat!(
            "===HOST_LOGS===\n",
            "__OXIDE_LOG_CAPABILITY__\tfull\tlinux_systemd\n",
            "JSON\t{\"__REALTIME_TIMESTAMP\":\"1732510800000000\",\"PRIORITY\":\"6\",\"SYSLOG_IDENTIFIER\":\"oxideterm-gateway\",\"_SYSTEMD_UNIT\":\"oxideterm-gateway.service\",\"MESSAGE\":\"WebSocket session demo@oxide.demo authenticated via Ed25519\"}\n",
            "JSON\t{\"__REALTIME_TIMESTAMP\":\"1732510740000000\",\"PRIORITY\":\"6\",\"SYSLOG_IDENTIFIER\":\"sshd\",\"_SYSTEMD_UNIT\":\"sshd.service\",\"MESSAGE\":\"Accepted publickey for demo from 10.0.12.5 port 52418 ssh2\"}\n",
            "JSON\t{\"__REALTIME_TIMESTAMP\":\"1732510680000000\",\"PRIORITY\":\"4\",\"SYSLOG_IDENTIFIER\":\"nginx\",\"_SYSTEMD_UNIT\":\"nginx.service\",\"MESSAGE\":\"upstream keepalive pool resized to 64 connections\"}\n",
            "JSON\t{\"__REALTIME_TIMESTAMP\":\"1732510520000000\",\"PRIORITY\":\"6\",\"SYSLOG_IDENTIFIER\":\"redis-server\",\"_SYSTEMD_UNIT\":\"redis-server.service\",\"MESSAGE\":\"Background saving terminated with success\"}\n",
            "JSON\t{\"__REALTIME_TIMESTAMP\":\"1732510300000000\",\"PRIORITY\":\"6\",\"SYSLOG_IDENTIFIER\":\"kernel\",\"_SYSTEMD_UNIT\":\"kernel\",\"MESSAGE\":\"eth0: Link is Up - 1Gbps/Full - flow control rx/tx\"}\n",
            "===HOST_LOGS_END===\n"
        )
        .to_string()
    } else if command.contains("===PORTS===") {
        concat!(
            "===PORTS===\n",
            "__OXIDE_PORT_CAPABILITY__\tfull\tlinux_ss\n",
            "SS\ttcp LISTEN 0 4096 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=404,fd=3))\n",
            "SS\ttcp LISTEN 0 511 0.0.0.0:80 0.0.0.0:* users:((\"nginx\",pid=942,fd=6))\n",
            "SS\ttcp LISTEN 0 511 0.0.0.0:443 0.0.0.0:* users:((\"nginx\",pid=942,fd=7))\n",
            "SS\ttcp LISTEN 0 128 127.0.0.1:6379 0.0.0.0:* users:((\"redis-server\",pid=718,fd=7))\n",
            "SS\ttcp LISTEN 0 1024 0.0.0.0:8443 0.0.0.0:* users:((\"oxideterm-gateway\",pid=1104,fd=9))\n",
            "SS\tudp UNCONN 0 0 127.0.0.53%lo:53 0.0.0.0:*\n",
            "SS\ttcp ESTAB 0 0 10.0.12.40:22 10.0.12.5:52418 users:((\"sshd\",pid=404,fd=4))\n",
            "===PORTS_END===\n"
        )
        .to_string()
    } else if command.contains("===TMUX===") {
        concat!(
            "===TMUX===\n",
            "__OXIDE_TMUX_CAPABILITY__\tfull\ttmux_cli\ttmux 3.4\n",
            "SESSION\t$1\toxide-deploy\t2\t1\t1732400000\t1732510800\n",
            "WINDOW\t$1\t@1\t0\tserver\t1\t2\tbb62,213x55,0,0[213x27,0,0,0,213x27,0,28,1]\t1732510800\n",
            "PANE\t$1\t@1\t%0\t0\t1\t1104\toxideterm-gateway\t/srv/oxideterm\t0\t1732510800\n",
            "PANE\t$1\t@1\t%1\t1\t0\t942\tjournalctl\t/var/log\t0\t1732510790\n",
            "SESSION\t$2\tdb-ops\t1\t0\t1732450000\t1732509000\n",
            "WINDOW\t$2\t@2\t0\tredis\t1\t1\tbb62,213x55,0,0,2\t1732509000\n",
            "PANE\t$2\t@2\t%2\t0\t1\t718\tredis-cli\t/home/demo\t0\t1732509000\n",
            "===TMUX_END===\n"
        )
        .to_string()
    } else if command.contains("===FILESYSTEMS===") {
        concat!(
            "===FILESYSTEMS===\n",
            "__OXIDE_FILESYSTEM_CAPABILITY__\tfull\tlinux_df\n",
            "DF\t/dev/nvme0n1p2\text4\t85899345920\t31138512896\t50410946560\t38\t/\n",
            "DF\t/dev/nvme0n1p3\txfs\t53687091200\t12884901888\t40802189312\t24\t/srv/oxideterm\n",
            "DF\t/dev/nvme0n1p4\text4\t21474836480\t17609365914\t3865470566\t82\t/var/log\n",
            "INODE\t/dev/nvme0n1p2\t5242880\t412030\t4830850\t8\t/\n",
            "INODE\t/dev/nvme0n1p3\t26214400\t194200\t26020200\t1\t/srv/oxideterm\n",
            "INODE\t/dev/nvme0n1p4\t1310720\t284110\t1026610\t22\t/var/log\n",
            "FINDMNT\tTARGET=\"/\" SOURCE=\"/dev/nvme0n1p2\" FSTYPE=\"ext4\" OPTIONS=\"rw,relatime\"\n",
            "FINDMNT\tTARGET=\"/srv/oxideterm\" SOURCE=\"/dev/nvme0n1p3\" FSTYPE=\"xfs\" OPTIONS=\"rw,noatime\"\n",
            "FINDMNT\tTARGET=\"/var/log\" SOURCE=\"/dev/nvme0n1p4\" FSTYPE=\"ext4\" OPTIONS=\"rw,nosuid,nodev\"\n",
            "LSBLK\tNAME=\"nvme0n1p2\" TYPE=\"part\" FSTYPE=\"ext4\" SIZE=\"85899345920\" MOUNTPOINTS=\"/\" MODEL=\"NVMe SSD\"\n",
            "===FILESYSTEMS_END===\n"
        )
        .to_string()
    } else if command.contains("===SCHEDULED_TASKS===") {
        concat!(
            "===SCHEDULED_TASKS===\n",
            "__OXIDE_SCHEDULE_CAPABILITY__\tfull\tlinux_systemd\n",
            "SYSTEMD\toxide-backup.timer\toxide-backup.service\tWed 2026-11-26 02:00:00 UTC\t14h left\tTue 2026-11-25 02:00:00 UTC\t9h ago\n",
            "SHOW\tId=oxide-backup.timer\tDescription=Nightly OxideTerm workspace snapshot\tUnit=oxide-backup.service\tActiveState=active\tUnitFileState=enabled\tResult=success\n",
            "SYSTEMD\tapt-daily.timer\tapt-daily.service\tWed 2026-11-26 06:00:00 UTC\t18h left\tTue 2026-11-25 06:00:00 UTC\t5h ago\n",
            "SHOW\tId=apt-daily.timer\tDescription=Daily apt download activities\tUnit=apt-daily.service\tActiveState=active\tUnitFileState=enabled\tResult=success\n",
            "CRON\t/etc/crontab\t0 3 * * * root /usr/sbin/logrotate /etc/logrotate.conf\n",
            "===SCHEDULED_TASKS_END===\n"
        )
        .to_string()
    } else if command.contains("===PACKAGES===") {
        concat!(
            "===PACKAGES===\n",
            "__OXIDE_PACKAGE_CAPABILITY__\tpartial\tlinux_packages\n",
            "MANAGER\tapt\ttrue\tapt\n",
            "ROW\topenssh-server\tapt\t1:9.6p1-3ubuntu13.4\t1:9.6p1-3ubuntu13.5\tamd64\tnoble-updates\tupgradable\t\t\t\tapt\n",
            "ROW\topenssl\tapt\t3.0.13-0ubuntu3.3\t3.0.13-0ubuntu3.4\tamd64\tnoble-security\tupgradable\t\t\t\tapt\n",
            "ROW\tnginx-core\tapt\t1.24.0-2ubuntu7.1\t1.24.0-2ubuntu7.2\tamd64\tnoble-updates\tupgradable\t\t\t\tapt\n",
            "===PACKAGES_END===\n"
        )
        .to_string()
    } else if command.contains("===REDIS") {
        concat!(
            "===REDIS===\n",
            "__OXIDE_REDIS_CAPABILITY__\tfull\tlinux_redis_cli\n",
            "ENDPOINT\thost:127.0.0.1:6379\thost\t127.0.0.1:6379\t127.0.0.1\t6379\ttrue\t7.2.5\tno_auth\n",
            "DB\thost:127.0.0.1:6379\t0\t128\t12\n",
            "===REDIS_END===\n"
        )
        .to_string()
    } else {
        String::new()
    }
}

struct SampleHost {
    sampling: Cell<bool>,
    gpu_sampling: Cell<bool>,
    history: Rc<RefCell<Vec<ResourceMetrics>>>,
    tick: Rc<Cell<u64>>,
}

impl SampleHost {
    fn build_metrics(step: u64, now: u64) -> ResourceMetrics {
        let phase = (step as f64) * 0.45;
        let cpu = (18.5 + phase.sin() * 6.5 + (phase * 0.5).cos() * 2.5).clamp(4.0, 92.0);
        let mem_gib = 3.0 + (phase * 0.3).sin() * 0.25;
        let mem_used = (mem_gib * ((1u64 << 30) as f64)) as u64;
        let mem_total = 8u64 << 30;
        let mem_pct = (mem_used as f64 / mem_total as f64) * 100.0;
        let rx = (148_200.0 + (phase * 1.1).sin().abs() * 210_000.0) as u64;
        let tx = (62_400.0 + (phase * 0.8).cos().abs() * 95_000.0) as u64;

        let mut m = ResourceMetrics::empty(now, MetricsSource::Full);
        m.system_info = Some(ResourceSystemInfo {
            system_name: Some("Ubuntu".into()),
            system_version: Some("24.04.2 LTS".into()),
            architecture: Some("x86_64".into()),
            boot_time_ms: Some(now.saturating_sub(428_400_000)),
            uptime_seconds: Some(428_400 + step * 2),
        });
        m.cpu_percent = Some(cpu);
        m.cpu_cores = Some(4);
        m.cpu_per_core = vec![
            ResourceCpuCore {
                index: 0,
                percent: Some((cpu + 4.5).clamp(2.0, 98.0)),
            },
            ResourceCpuCore {
                index: 1,
                percent: Some((cpu - 3.2).clamp(2.0, 98.0)),
            },
            ResourceCpuCore {
                index: 2,
                percent: Some((cpu + 1.4).clamp(2.0, 98.0)),
            },
            ResourceCpuCore {
                index: 3,
                percent: Some((cpu - 2.7).clamp(2.0, 98.0)),
            },
        ];
        m.load_avg_1 = Some((0.42 + phase.sin() * 0.12).max(0.05));
        m.load_avg_5 = Some(0.38);
        m.load_avg_15 = Some(0.31);
        m.memory_total = Some(mem_total);
        m.memory_used = Some(mem_used);
        m.memory_percent = Some(mem_pct);
        m.memory_buffers = Some(256 << 20);
        m.memory_cached = Some(1536 << 20);
        m.swap_total = Some(2 << 30);
        m.swap_used = Some(128 << 20);
        m.swap_percent = Some(6.25);
        m.disk_total = Some(80 << 30);
        m.disk_used = Some(29 << 30);
        m.disk_percent = Some(36.25);
        m.disks = vec![
            ResourceDisk {
                mount_point: "/".into(),
                used: 29 << 30,
                total: 80 << 30,
                percent: Some(36.25),
            },
            ResourceDisk {
                mount_point: "/srv/oxideterm".into(),
                used: 12 << 30,
                total: 50 << 30,
                percent: Some(24.0),
            },
        ];
        m.net_rx_bytes_per_sec = Some(rx);
        m.net_tx_bytes_per_sec = Some(tx);
        m.net_interfaces = vec![ResourceNetInterface {
            name: "eth0".into(),
            rx_bytes: 18_420_112_384 + step * rx,
            tx_bytes: 6_218_940_416 + step * tx,
            rx_bytes_per_sec: Some(rx),
            tx_bytes_per_sec: Some(tx),
        }];
        m.ssh_rtt_ms = Some(12 + (step % 5));
        m.top_processes = vec![
            ResourceTopProcess {
                pid: "1104".into(),
                ppid: Some("1".into()),
                user: Some("oxide".into()),
                state: Some("S".into()),
                cpu_percent: Some((6.8 + phase.sin() * 1.4).max(0.5)),
                memory_percent: 4.2,
                rss_bytes: Some(344 << 20),
                vsz_bytes: Some(1280 << 20),
                elapsed: Some("4-23:00:12".into()),
                command: "oxideterm-gateway".into(),
                full_command: Some("/usr/local/bin/oxideterm-gateway --listen :8443".into()),
            },
            ResourceTopProcess {
                pid: "942".into(),
                ppid: Some("1".into()),
                user: Some("www-data".into()),
                state: Some("S".into()),
                cpu_percent: Some((2.4 + (phase * 0.7).cos().abs()).max(0.2)),
                memory_percent: 1.8,
                rss_bytes: Some(148 << 20),
                vsz_bytes: Some(512 << 20),
                elapsed: Some("4-23:00:08".into()),
                command: "nginx".into(),
                full_command: Some("nginx: worker process".into()),
            },
            ResourceTopProcess {
                pid: "718".into(),
                ppid: Some("1".into()),
                user: Some("redis".into()),
                state: Some("S".into()),
                cpu_percent: Some(0.9),
                memory_percent: 1.1,
                rss_bytes: Some(90 << 20),
                vsz_bytes: Some(280 << 20),
                elapsed: Some("4-23:00:10".into()),
                command: "redis-server".into(),
                full_command: Some("/usr/bin/redis-server 127.0.0.1:6379".into()),
            },
            ResourceTopProcess {
                pid: "404".into(),
                ppid: Some("1".into()),
                user: Some("root".into()),
                state: Some("S".into()),
                cpu_percent: Some(0.5),
                memory_percent: 0.4,
                rss_bytes: Some(32 << 20),
                vsz_bytes: Some(124 << 20),
                elapsed: Some("4-23:00:15".into()),
                command: "sshd".into(),
                full_command: Some("sshd: /usr/sbin/sshd -D".into()),
            },
        ];
        m.docker.status = ResourceDockerStatus::Available;
        m.docker.containers.push(ResourceDockerContainer {
            id: "a84c2f91e01b".into(),
            name: "oxide-web".into(),
            image: "nginx:alpine".into(),
            state: "running".into(),
            status: "Up 4 days".into(),
            ports: Some("0.0.0.0:8080->80/tcp".into()),
        });
        m.docker.containers.push(ResourceDockerContainer {
            id: "d19e7b30c44a".into(),
            name: "oxide-redis".into(),
            image: "redis:7-alpine".into(),
            state: "running".into(),
            status: "Up 4 days".into(),
            ports: Some("127.0.0.1:6379->6379/tcp".into()),
        });
        m.services.status = ResourceServiceStatus::Available {
            platform: "linux".into(),
            capability: Default::default(),
        };
        m.services.services.push(ResourceService {
            id: "sshd.service".into(),
            load_state: "loaded".into(),
            active_state: "active".into(),
            sub_state: "running".into(),
            enabled_state: "enabled".into(),
            main_pid: Some("404".into()),
            description: "OpenSSH server daemon".into(),
        });
        m.services.services.push(ResourceService {
            id: "nginx.service".into(),
            load_state: "loaded".into(),
            active_state: "active".into(),
            sub_state: "running".into(),
            enabled_state: "enabled".into(),
            main_pid: Some("942".into()),
            description: "A high performance web server".into(),
        });
        m.services.services.push(ResourceService {
            id: "redis-server.service".into(),
            load_state: "loaded".into(),
            active_state: "active".into(),
            sub_state: "running".into(),
            enabled_state: "enabled".into(),
            main_pid: Some("718".into()),
            description: "Advanced key-value store".into(),
        });
        m
    }
}

impl HostToolsBridge for SampleHost {
    fn pool_snapshot(&self) -> HostPoolSnapshot {
        // Sample metrics don't represent an SSH pool owned by this browser.
        HostPoolSnapshot { stats: None, summaries: Vec::new(), topology: None }
    }
    fn connections(&self) -> Vec<MonitorConnectionOption> {
        vec![MonitorConnectionOption {
            connection_id: "demo".into(),
            host: "oxide.demo".into(),
            port: 22,
            username: "demo".into(),
            label: Some("OxideTerm".into()),
        }]
    }
    fn os_type(&self, id: &str) -> Option<String> {
        (id == "demo").then(|| "linux".into())
    }
    fn is_local(&self, _: &str) -> bool {
        false
    }
    fn detecting_os(&self, _: &str) -> bool {
        false
    }
    fn cancel_captures(&self, _: Option<&str>) {}
    fn target(&self, id: &str) -> Option<HostCaptureTarget> {
        (id == "demo").then(|| Rc::new(DemoHostCapture) as HostCaptureTarget)
    }
    fn profiler_snapshot(&self, id: &str) -> Option<ConnectionProfilerSnapshot> {
        if id != "demo" {
            return None;
        }
        let history = self.history.borrow().clone();
        let metrics = history
            .last()
            .cloned()
            .unwrap_or_else(|| Self::build_metrics(self.tick.get(), js_sys::Date::now() as u64));
        Some(ConnectionProfilerSnapshot {
            history,
            metrics: Some(metrics),
            state: if self.sampling.get() {
                ProfilerState::Running
            } else {
                ProfilerState::Stopped
            },
            failure: None,
        })
    }
    fn profiler_ids(&self) -> Vec<String> {
        vec!["demo".into()]
    }
    fn start_profiler(&self, _: &str, _: ResourceSamplingConfig, u: HostSamplerUpdates) {
        self.sampling.set(true);
        u.profiler_changed();
    }
    fn stop_profiler(&self, _: &str) {
        self.sampling.set(false);
    }
    fn refresh_profiler(&self, _: &str) {}
    fn start_gpu(&self, id: &str, updates: HostSamplerUpdates) -> bool {
        self.gpu_sampling.set(true);
        updates.gpu_updated(GpuUpdate {
            connection_id: id.to_string(),
            snapshot: GpuSnapshot {
                timestamp_ms: js_sys::Date::now() as u64,
                status: GpuSnapshotStatus::Available,
                devices: vec![GpuDevice {
                    provider: GpuProvider::Nvidia,
                    index: 0,
                    uuid: "GPU-oxide-demo-0".into(),
                    pci_bus_id: "00000000:01:00.0".into(),
                    name: "NVIDIA L40S".into(),
                    driver_version: Some("550.90.07".into()),
                    performance_state: Some("P2".into()),
                    health_status: None,
                    utilization_percent: Some(24.0),
                    memory_utilization_percent: Some(12.5),
                    memory_used: Some(6 << 30),
                    memory_total: Some(48 << 30),
                    temperature_celsius: Some(46.0),
                    power_draw_watts: Some(92.0),
                    power_limit_watts: Some(300.0),
                    fan_speed_percent: Some(34.0),
                }],
                processes: vec![],
            },
        });
        true
    }
    fn gpu_running(&self, _: &str) -> bool {
        self.gpu_sampling.get()
    }
    fn stop_gpu(&self, _: &str) {
        self.gpu_sampling.set(false);
    }
}

pub struct Monitor {
    entity: Entity<HostToolsEntity>,
    focus: FocusHandle,
    pub width: f32,
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub monitoring: HostToolsSettings,
    _sampler: Task<()>,
}

impl Monitor {
    pub fn active_tool(&self, cx: &App) -> ContextSidebarTool {
        self.entity.read(cx).active_tool()
    }

    pub(crate) fn graph_controls(&self) -> Entity<HostToolsEntity> { self.entity.clone() }

    pub fn tool_enabled(&self, tool: ContextSidebarTool) -> bool {
        tool.monitoring_enabled(&self.monitoring)
    }

    pub fn set_tool_enabled(
        &mut self,
        tool: ContextSidebarTool,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        tool.set_monitoring_enabled(&mut self.monitoring, enabled);
        self.sync_lifecycle(cx);
        cx.notify();
    }

    pub fn select(&mut self, tool: ContextSidebarTool, cx: &mut Context<Self>) {
        self.entity
            .update(cx, |e, cx| e.select_sidebar_tool(tool, cx));
        self.sync_lifecycle(cx);
        cx.notify();
    }

    fn sync_lifecycle(&self, cx: &mut Context<Self>) {
        let runtime = HostToolsRuntime(cx.foreground_executor().clone());
        let monitoring = self.monitoring.clone();
        self.entity.update(cx, |e, cx| {
            e.update_lifecycle(
                HostToolsVisibility::VisibleSidebar,
                monitoring,
                ResourceSamplingConfig::default(),
                runtime,
                false,
                cx,
            );
        });
    }

    pub fn new(tokens: ThemeTokens, i18n: I18n, cx: &mut Context<Self>) -> Self {
        let monitoring = HostToolsSettings::default();
        let runtime = HostToolsRuntime(cx.foreground_executor().clone());
        let now = js_sys::Date::now() as u64;
        let initial_history: Vec<ResourceMetrics> = (0..12u64)
            .map(|i| SampleHost::build_metrics(i, now.saturating_sub((12 - i) * 2000)))
            .collect();
        let history = Rc::new(RefCell::new(initial_history));
        let tick = Rc::new(Cell::new(12u64));
        let sampler_history = history.clone();
        let sampler_tick = tick.clone();

        let entity = cx.new(|cx| {
            let mut entity = HostToolsEntity::with_bridge(
                Box::new(SampleHost {
                    sampling: Cell::new(true),
                    gpu_sampling: Cell::new(false),
                    history,
                    tick,
                }),
                cx,
            );
            entity.update_lifecycle(
                HostToolsVisibility::VisibleSidebar,
                monitoring.clone(),
                ResourceSamplingConfig::default(),
                runtime,
                true,
                cx,
            );
            entity
        });
        cx.observe(&entity, |_, _, cx| cx.notify()).detach();

        // Advance the live demo history ring buffer every 2s so CPU/memory/net
        // charts update continuously like a live remote host.
        let sampler = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                let next = sampler_tick.get().wrapping_add(1);
                sampler_tick.set(next);
                let sample = SampleHost::build_metrics(next, js_sys::Date::now() as u64);
                {
                    let mut h = sampler_history.borrow_mut();
                    h.push(sample);
                    if h.len() > HISTORY_CAP {
                        let overflow = h.len() - HISTORY_CAP;
                        h.drain(0..overflow);
                    }
                }
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });

        Self {
            entity,
            focus: cx.focus_handle(),
            width: oxideterm_settings::AI_SIDEBAR_DEFAULT_WIDTH as f32,
            tokens,
            i18n,
            monitoring,
            _sampler: sampler,
        }
    }

    fn key(&mut self, event: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.entity.read(cx).ui.focused_input else {
            return;
        };
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.entity.update(cx, |e, cx| {
                e.blur_input(cx);
            });
        } else if key == "backspace" {
            self.entity.update(cx, |e, cx| {
                let mut value = current_input_value(e, input);
                let len = value.encode_utf16().count();
                value.pop();
                e.replace_text_input(input, Some(0..len), &value);
                cx.notify();
            });
        } else if let Some(text) = &event.keystroke.key_char
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.platform
            && !text.chars().any(char::is_control)
        {
            self.entity.update(cx, |e, cx| {
                let mut value = current_input_value(e, input);
                let len = value.encode_utf16().count();
                value.push_str(text);
                e.replace_text_input(input, Some(0..len), &value);
                cx.notify();
            });
        } else {
            return;
        }
        w.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }

    fn plain_ime(&self, input: HostToolsTextInput, cx: &App) -> HostToolsPlainTextImeFrame {
        let focused = self.entity.read(cx).ui.focused_input == Some(input);
        HostToolsPlainTextImeFrame {
            input,
            anchor_id: TextInputAnchorId(0),
            viewport: TextInputViewport::default(),
            caret_visible: focused,
            selected_range: None,
            active_offset: None,
            marked_text: None,
            update_anchor: Rc::new(|_| {}),
        }
    }

    fn render_disabled(&self, tool: ContextSidebarTool, cx: &mut Context<Self>) -> AnyElement {
        let color = self.tokens.ui.text_muted;
        let border = rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA);
        let hover_bg = rgb(self.tokens.ui.bg_hover);
        div()
            .size_full()
            .p_4()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .text_align(TextAlign::Center)
            .text_color(rgb(color))
            .child(
                div().mb_2().opacity(0.3).child(
                    svg()
                        .path(oxideterm_gpui_ui::icons::LucideIcon::Power.path())
                        .size(px(24.0))
                        .text_color(rgb(color)),
                ),
            )
            .child(
                div()
                    .mb_3()
                    .text_size(px(14.0))
                    .child(self.i18n.t("profiler.panel.disabled")),
            )
            .child(
                div()
                    .id(("web-host-tool-enable", tool as usize))
                    .px_3()
                    .py_1()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(border)
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .hover(move |b| b.bg(hover_bg))
                    .child(self.i18n.t("profiler.panel.enable"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.set_tool_enabled(tool, true, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }
}

fn current_input_value(e: &HostToolsEntity, input: HostToolsTextInput) -> String {
    match input {
        HostToolsTextInput::ProcessSearch => e.ui.host_process_search_query.clone(),
        HostToolsTextInput::ProcessRenice => e.ui.host_process_renice_value.clone(),
        HostToolsTextInput::DockerSearch => e.ui.host_docker_search_query.clone(),
        HostToolsTextInput::ContainerSearch => e.ui.host_container_search_query.clone(),
        HostToolsTextInput::ServiceSearch => e.ui.host_service_search_query.clone(),
        HostToolsTextInput::LogSearch => e.ui.host_log_search_query.clone(),
        HostToolsTextInput::PortSearch => e.ui.host_port_search_query.clone(),
        HostToolsTextInput::TmuxSearch => e.ui.host_tmux_search_query.clone(),
        HostToolsTextInput::FilesystemSearch => {
            e.filesystem_search_value().unwrap_or_default().to_owned()
        }
        HostToolsTextInput::ScheduleSearch => e.ui.host_schedule_search_query.clone(),
        HostToolsTextInput::PackageSearch => e.ui.host_package_search_query.clone(),
        HostToolsTextInput::RedisPassword => e.ui.host_redis_password.as_str().to_owned(),
        HostToolsTextInput::RedisSearch => e.ui.host_redis_search_query.clone(),
        _ => String::new(),
    }
}

impl Render for Monitor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_tool = self.entity.read(cx).active_tool();
        if !self.tool_enabled(active_tool) {
            return div()
                .id("web-monitor")
                .size_full()
                .min_h_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(self.render_disabled(active_tool, cx));
        }

        let tokens = self.tokens;
        let i18n = self.i18n.clone();
        let mono_font: SharedString = "Lilex".into();
        let selectable_text = HostToolsText::new(|_, _, _, s, c, _| {
            div().text_color(rgb(c)).child(s).into_any_element()
        });
        let width = self.width;
        let frame = MonitorRenderContext {
            tokens,
            i18n: i18n.clone(),
            mono_font_family: mono_font.clone(),
            selectable_text: selectable_text.clone(),
            sidebar_width: width,
        };

        let content = match active_tool {
            ContextSidebarTool::Monitor => self
                .entity
                .update(cx, |s, cx| s.render_host_monitor_panel(&frame, true, cx)),
            ContextSidebarTool::Gpu => self.entity.update(cx, |s, cx| {
                s.render_host_gpu_panel(&tokens, &i18n, mono_font, &selectable_text, cx)
            }),
            ContextSidebarTool::Processes => {
                let search = self.plain_ime(HostToolsTextInput::ProcessSearch, cx);
                let renice = self.plain_ime(HostToolsTextInput::ProcessRenice, cx);
                self.entity.update(cx, |s, cx| {
                    s.render_host_processes_panel(
                        search,
                        renice,
                        width,
                        &tokens,
                        &i18n,
                        mono_font,
                        &selectable_text,
                        cx,
                    )
                })
            }
            ContextSidebarTool::Docker => {
                let search = self.plain_ime(HostToolsTextInput::DockerSearch, cx);
                self.entity.update(cx, |s, cx| {
                    s.render_host_docker_panel(
                        search,
                        false,
                        width,
                        &tokens,
                        &i18n,
                        mono_font,
                        &selectable_text,
                        cx,
                    )
                })
            }
            ContextSidebarTool::Container => {
                let search = self.plain_ime(HostToolsTextInput::ContainerSearch, cx);
                self.entity.update(cx, |s, cx| {
                    s.render_host_container_panel(
                        search,
                        &tokens,
                        &i18n,
                        mono_font,
                        &selectable_text,
                        cx,
                    )
                })
            }
            ContextSidebarTool::Services => {
                let search = self.plain_ime(HostToolsTextInput::ServiceSearch, cx);
                self.entity.update(cx, |s, cx| {
                    s.render_host_services_panel(
                        search,
                        false,
                        width,
                        &tokens,
                        &i18n,
                        mono_font,
                        &selectable_text,
                        cx,
                    )
                })
            }
            ContextSidebarTool::Logs => {
                let search = self.plain_ime(HostToolsTextInput::LogSearch, cx);
                self.entity.update(cx, |s, cx| {
                    s.render_host_logs_panel(
                        search,
                        &tokens,
                        &i18n,
                        mono_font,
                        &selectable_text,
                        width >= HOST_LOG_CONTEXT_COLUMNS_MIN_WIDTH,
                        cx,
                    )
                })
            }
            ContextSidebarTool::Ports => {
                let search = self.plain_ime(HostToolsTextInput::PortSearch, cx);
                self.entity.update(cx, |s, cx| {
                    s.render_host_ports_panel(
                        search,
                        width,
                        &tokens,
                        &i18n,
                        mono_font,
                        &selectable_text,
                        cx,
                    )
                })
            }
            ContextSidebarTool::Tmux => {
                let search = self.plain_ime(HostToolsTextInput::TmuxSearch, cx);
                self.entity.update(cx, |s, cx| {
                    s.render_host_tmux_panel(
                        search,
                        width,
                        false,
                        &tokens,
                        &i18n,
                        mono_font,
                        &selectable_text,
                        cx,
                    )
                })
            }
            ContextSidebarTool::Filesystems => {
                let focused = self.entity.read(cx).ui.focused_input
                    == Some(HostToolsTextInput::FilesystemSearch);
                let search = self
                    .plain_ime(HostToolsTextInput::FilesystemSearch, cx)
                    .into_filesystem_search(focused);
                self.entity.update(cx, |s, cx| {
                    s.render_filesystems_panel(
                        search,
                        width,
                        &tokens,
                        &i18n,
                        mono_font,
                        &selectable_text,
                        cx,
                    )
                })
            }
            ContextSidebarTool::Schedules => {
                let search = self.plain_ime(HostToolsTextInput::ScheduleSearch, cx);
                self.entity.update(cx, |s, cx| {
                    s.render_host_schedules_panel(
                        search,
                        width,
                        &tokens,
                        &i18n,
                        mono_font,
                        &selectable_text,
                        cx,
                    )
                })
            }
            ContextSidebarTool::Packages => {
                let search = self.plain_ime(HostToolsTextInput::PackageSearch, cx);
                self.entity.update(cx, |s, cx| {
                    s.render_host_packages_panel(
                        search,
                        width,
                        &tokens,
                        &i18n,
                        mono_font,
                        &selectable_text,
                        cx,
                    )
                })
            }
            ContextSidebarTool::Redis => {
                let password = self.plain_ime(HostToolsTextInput::RedisPassword, cx);
                let search = self.plain_ime(HostToolsTextInput::RedisSearch, cx);
                self.entity.update(cx, |s, cx| {
                    s.render_host_redis_panel(
                        password,
                        search,
                        &tokens,
                        &i18n,
                        mono_font,
                        &selectable_text,
                        cx,
                    )
                })
            }
        };

        div()
            .id("web-monitor")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .track_focus(&self.focus)
            .capture_any_mouse_down(cx.listener(|this, _, w, cx| {
                w.focus(&this.focus, cx);
            }))
            .on_key_down(cx.listener(Self::key))
            .child(content)
    }
}
