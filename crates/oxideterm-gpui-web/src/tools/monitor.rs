use gpui::{prelude::*, *};
use oxideterm_connection_monitor::*;
use oxideterm_gpui_host_tools::*;
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use oxideterm_topology::*;
use std::cell::Cell;

struct SampleHost {
    started: web_time::SystemTime,
    sampling: Cell<bool>,
}
impl HostToolsBridge for SampleHost {
    fn pool_snapshot(&self) -> HostPoolSnapshot {
        let node = ConnectionTopologyNode {
            connection_id: "demo".into(),
            parent_connection_id: None,
            host: "oxide.demo".into(),
            port: 22,
            username: "demo".into(),
            status: ConnectionTopologyStatus::Active,
            depth: 0,
            ref_count: 2,
            consumers: ConnectionTopologyConsumerSummary {
                node_router: 1,
                port_forwards: 1,
                ..Default::default()
            },
        };
        HostPoolSnapshot {
            stats: Some(ConnectionPoolMonitorStats {
                total_connections: 1,
                active_connections: 1,
                total_ref_count: 2,
                ..Default::default()
            }),
            summaries: vec![ConnectionPoolEntrySummary {
                id: node.connection_id.clone(),
                host: node.host.clone(),
                port: 22,
                username: "demo".into(),
                state: ConnectionPoolEntryState::Active,
                ref_count: 2,
                keep_alive: true,
                created_at: self.started,
                last_active_at: self.started,
                terminal_count: 0,
                has_sftp_session: false,
                forward_count: 1,
                parent_connection_id: None,
            }],
            topology: Some(ConnectionTopologySnapshot::new(vec![node], vec![])),
        }
    }
    fn connections(&self) -> Vec<MonitorConnectionOption> {
        vec![MonitorConnectionOption {
            connection_id: "demo".into(),
            host: "oxide.demo".into(),
            port: 22,
            username: "demo".into(),
            label: None,
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
    fn target(&self, _: &str) -> Option<HostCaptureTarget> {
        None
    }
    fn profiler_snapshot(&self, id: &str) -> Option<ConnectionProfilerSnapshot> {
        if id != "demo" {
            return None;
        }
        let mut m = ResourceMetrics::empty(js_sys::Date::now() as u64, MetricsSource::Full);
        m.cpu_percent = Some(18.5);
        m.cpu_cores = Some(4);
        m.memory_total = Some(8 << 30);
        m.memory_used = Some(3 << 30);
        m.memory_percent = Some(37.5);
        m.docker.status = ResourceDockerStatus::Available;
        m.docker.containers.push(ResourceDockerContainer {
            id: "oxide-web".into(),
            name: "web".into(),
            image: "nginx:alpine".into(),
            state: "running".into(),
            status: "Up".into(),
            ports: Some("8080:80".into()),
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
            description: "OpenSSH".into(),
        });
        Some(ConnectionProfilerSnapshot {
            history: vec![],
            metrics: Some(m),
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
    fn start_gpu(&self, _: &str, _: HostSamplerUpdates) -> bool {
        false
    }
    fn gpu_running(&self, _: &str) -> bool {
        false
    }
    fn stop_gpu(&self, _: &str) {}
}
pub struct Monitor {
    entity: Entity<HostToolsEntity>,
    pub width: f32,
    pub tokens: ThemeTokens,
    pub i18n: I18n,
}
impl Monitor {
    pub fn active_tool(&self, cx: &App) -> ContextSidebarTool {
        self.entity.read(cx).active_tool()
    }
    pub fn select(&self, tool: ContextSidebarTool, cx: &mut App) {
        self.entity.update(cx, |e, cx| e.select_tool(tool, cx));
    }
    pub fn new(tokens: ThemeTokens, i18n: I18n, cx: &mut Context<Self>) -> Self {
        let entity = cx.new(|cx| {
            HostToolsEntity::with_bridge(
                Box::new(SampleHost {
                    started: web_time::SystemTime::now(),
                    sampling: Cell::new(true),
                }),
                cx,
            )
        });
        cx.observe(&entity, |_, _, cx| cx.notify()).detach();
        Self {
            entity,
            width: oxideterm_settings::AI_SIDEBAR_DEFAULT_WIDTH as f32,
            tokens,
            i18n,
        }
    }
}
impl Render for Monitor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let frame = MonitorRenderContext {
            tokens: self.tokens,
            i18n: self.i18n.clone(),
            mono_font_family: "Lilex".into(),
            selectable_text: HostToolsText::new(|_, _, _, s, c, _| {
                div().text_color(rgb(c)).child(s).into_any_element()
            }),
            sidebar_width: self.width,
        };
        div()
            .id("web-monitor")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.entity.update(cx, |s, cx| match s.active_tool() {
                ContextSidebarTool::Docker => s.render_sampled_docker_list(&frame, cx),
                ContextSidebarTool::Services => s.render_sampled_service_list(&frame, cx),
                ContextSidebarTool::Monitor => s.render_host_monitor_panel(&frame, true, cx),
                _ => unreachable!("only sampled tools are mounted"),
            }))
    }
}
