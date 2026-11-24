use super::*;
use crate::delivery::HostToolsSamplerDelivery;
use oxideterm_connection_monitor::{ConnectionProfilerSnapshot, ResourceSamplingConfig};
use oxideterm_topology::ConnectionTopologySnapshot;
use oxideterm_workspace::delivery::ActiveDeliverySender;
use zeroize::Zeroizing;

#[cfg(not(target_family = "wasm"))]
pub use oxideterm_ssh::{
    SshCommandOutput as HostCommandOutput, SshSecretCommandOutput as HostSecretCommandOutput,
};
#[cfg(not(target_family = "wasm"))]
pub type HostToolsRuntime = tokio::runtime::Handle;
#[cfg(not(target_family = "wasm"))]
pub type HostCaptureTask = tokio::task::JoinHandle<()>;

#[cfg(target_family = "wasm")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostCommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub truncated: bool,
}

#[cfg(target_family = "wasm")]
pub struct HostSecretCommandOutput {
    pub stdout: Zeroizing<Vec<u8>>,
    pub stderr: Zeroizing<Vec<u8>>,
    pub exit_code: Option<i32>,
    pub truncated: bool,
}

#[cfg(target_family = "wasm")]
#[derive(Clone)]
pub struct HostToolsRuntime(pub gpui::ForegroundExecutor);
#[cfg(target_family = "wasm")]
pub type HostCaptureTask = futures_util::future::AbortHandle;

#[cfg(target_family = "wasm")]
impl HostToolsRuntime {
    pub(crate) fn spawn(&self, future: impl Future<Output = ()> + 'static) -> HostCaptureTask {
        let (task, handle) = futures_util::future::abortable(future);
        self.0
            .spawn(async move {
                let _ = task.await;
            })
            .detach();
        handle
    }
}

#[cfg(not(target_family = "wasm"))]
pub type HostCaptureFuture<'a, T> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, ()>> + Send + 'a>>;
#[cfg(target_family = "wasm")]
pub type HostCaptureFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = Result<T, ()>> + 'a>>;

#[cfg(not(target_family = "wasm"))]
pub type HostCaptureTarget = Arc<dyn HostCapture + Send + Sync>;
#[cfg(target_family = "wasm")]
pub type HostCaptureTarget = std::rc::Rc<dyn HostCapture>;

/// A connection-scoped capability. Dropping a pending future cancels that capture,
/// not the shared connection.
pub trait HostCapture {
    fn run_command_capture<'a>(
        &'a self,
        command: &'a str,
        timeout: Duration,
        max_output_size: usize,
    ) -> HostCaptureFuture<'a, HostCommandOutput>;
    fn run_secret_command_capture_with_stdin<'a>(
        &'a self,
        command: &'a str,
        stdin: Zeroizing<Vec<u8>>,
        timeout: Duration,
        max_output_size: usize,
    ) -> HostCaptureFuture<'a, HostSecretCommandOutput>;
}

#[cfg(not(target_family = "wasm"))]
impl HostCapture for HostCommandTarget {
    fn run_command_capture<'a>(
        &'a self,
        command: &'a str,
        timeout: Duration,
        max_output_size: usize,
    ) -> HostCaptureFuture<'a, HostCommandOutput> {
        Box::pin(self.run_command_capture(command, timeout, max_output_size))
    }
    fn run_secret_command_capture_with_stdin<'a>(
        &'a self,
        command: &'a str,
        stdin: Zeroizing<Vec<u8>>,
        timeout: Duration,
        max_output_size: usize,
    ) -> HostCaptureFuture<'a, HostSecretCommandOutput> {
        Box::pin(self.run_secret_command_capture_with_stdin(
            command,
            stdin,
            timeout,
            max_output_size,
        ))
    }
}

pub struct HostPoolSnapshot {
    pub stats: Option<ConnectionPoolMonitorStats>,
    pub summaries: Vec<ConnectionPoolEntrySummary>,
    pub topology: Option<ConnectionTopologySnapshot>,
}

#[derive(Clone)]
pub struct HostSamplerUpdates(pub(crate) ActiveDeliverySender<HostToolsSamplerDelivery>);

impl HostSamplerUpdates {
    pub fn profiler_changed(&self) {
        let _ = self
            .0
            .send(delivery::HostToolsSamplerDelivery::ProfilerChanged);
    }
    pub fn gpu_updated(&self, update: GpuUpdate) {
        let _ = self
            .0
            .send(delivery::HostToolsSamplerDelivery::GpuUpdated(update));
    }
}

/// Each entity owns its subscriptions. Stopping one must not release the host transport.
pub trait HostToolsBridge {
    fn pool_snapshot(&self) -> HostPoolSnapshot;
    fn connections(&self) -> Vec<MonitorConnectionOption>;
    fn os_type(&self, id: &str) -> Option<String>;
    fn is_local(&self, id: &str) -> bool;
    fn detecting_os(&self, id: &str) -> bool;
    /// Invalidate previously issued capture capabilities for this host, including
    /// captures that have been submitted but not polled yet. None retires the page.
    fn cancel_captures(&self, id: Option<&str>);
    fn target(&self, id: &str) -> Option<HostCaptureTarget>;
    fn profiler_snapshot(&self, id: &str) -> Option<ConnectionProfilerSnapshot>;
    fn profiler_ids(&self) -> Vec<String>;
    fn start_profiler(&self, id: &str, config: ResourceSamplingConfig, updates: HostSamplerUpdates);
    fn refresh_profiler(&self, id: &str);
    fn stop_profiler(&self, id: &str);
    fn start_gpu(&self, id: &str, updates: HostSamplerUpdates) -> bool;
    fn gpu_running(&self, id: &str) -> bool;
    fn stop_gpu(&self, id: &str);
}
