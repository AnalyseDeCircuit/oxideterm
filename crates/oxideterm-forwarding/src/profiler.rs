// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
        mpsc::Sender,
    },
    time::Duration,
};

use oxideterm_ssh::{ConnectionState, ConnectionTransportStatus, SshConnectionHandle};
use tokio::{sync::oneshot, task::JoinHandle};

use crate::{
    DetectedPort, ForwardEvent, PortDetectionSnapshot,
    detection::{
        PORT_SCAN_MAX_OUTPUT_SIZE, PORT_SCAN_TIMEOUT_SECS, REMOTE_OS_PROBE_TIMEOUT_SECS,
        REMOTE_OS_PROBE_UNIX, REMOTE_OS_PROBE_WINDOWS, RemotePortScanPlatform,
    },
};

const PROFILER_INTERVAL: Duration = Duration::from_secs(10);
const PROFILER_CHANNEL_OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const PROFILER_SAMPLE_TIMEOUT: Duration = Duration::from_secs(PORT_SCAN_TIMEOUT_SECS);
const PROFILER_END_MARKER: &str = "===END===";
const PROFILER_MAX_CONSECUTIVE_FAILURES: u32 = 3;
const PROFILER_INIT_UNIX: &str =
    "export PS1=''; export PS2=''; stty -echo 2>/dev/null; export LANG=C\n";
const PROFILER_INIT_WINDOWS: &str = "set PROMPT=\r\n";

#[derive(Debug)]
pub struct PortDetectionProfiler {
    connection_id: String,
    detected_ports: Arc<Mutex<Vec<DetectedPort>>>,
    ignored_ports: Arc<Mutex<HashSet<u16>>>,
    lifecycle: Arc<AtomicU8>,
    stop_tx: Mutex<Option<oneshot::Sender<()>>>,
    task: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProfilerLifecycle {
    Running,
    Degraded,
    Stopped,
}

impl ProfilerLifecycle {
    const fn as_u8(self) -> u8 {
        match self {
            Self::Running => 0,
            Self::Degraded => 1,
            Self::Stopped => 2,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Running,
            1 => Self::Degraded,
            _ => Self::Stopped,
        }
    }
}

impl PortDetectionProfiler {
    pub fn spawn(
        connection_id: String,
        ssh_connection: SshConnectionHandle,
        event_tx: Sender<ForwardEvent>,
    ) -> Self {
        let detected_ports = Arc::new(Mutex::new(Vec::new()));
        let ignored_ports = Arc::new(Mutex::new(HashSet::new()));
        let lifecycle = Arc::new(AtomicU8::new(ProfilerLifecycle::Running.as_u8()));
        let (stop_tx, stop_rx) = oneshot::channel();
        let task_detected_ports = detected_ports.clone();
        let task_ignored_ports = ignored_ports.clone();
        let task_lifecycle = lifecycle.clone();
        let task_connection_id = connection_id.clone();
        let task = tokio::spawn(async move {
            profiler_loop(
                task_connection_id,
                ssh_connection,
                event_tx,
                task_detected_ports,
                task_ignored_ports,
                task_lifecycle,
                stop_rx,
            )
            .await;
        });

        Self {
            connection_id,
            detected_ports,
            ignored_ports,
            lifecycle,
            stop_tx: Mutex::new(Some(stop_tx)),
            task: Mutex::new(Some(task)),
        }
    }

    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub fn is_running(&self) -> bool {
        self.lifecycle() == ProfilerLifecycle::Running
    }

    pub fn is_stopped(&self) -> bool {
        self.lifecycle() == ProfilerLifecycle::Stopped
    }

    pub fn is_degraded(&self) -> bool {
        self.lifecycle() == ProfilerLifecycle::Degraded
    }

    fn lifecycle(&self) -> ProfilerLifecycle {
        ProfilerLifecycle::from_u8(self.lifecycle.load(Ordering::Acquire))
    }

    pub fn detected_ports(&self) -> Vec<DetectedPort> {
        self.detected_ports
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn snapshot(&self) -> PortDetectionSnapshot {
        PortDetectionSnapshot {
            new_ports: Vec::new(),
            closed_ports: Vec::new(),
            all_ports: self.detected_ports(),
            // Tauri's hook treats any successful getDetectedPorts call as a
            // completed poll, even before the first silent profiler baseline.
            // Keep the internal baseline flag separate from this UI-facing bit.
            has_scanned: true,
        }
    }

    pub fn ignore_port(&self, port: u16) {
        self.ignored_ports
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(port);
    }

    pub fn stop(&self) {
        self.lifecycle
            .store(ProfilerLifecycle::Stopped.as_u8(), Ordering::Release);
        if let Some(tx) = self
            .stop_tx
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            let _ = tx.send(());
        }
        if let Some(task) = self
            .task
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            task.abort();
        }
    }
}

impl Drop for PortDetectionProfiler {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn profiler_loop(
    connection_id: String,
    ssh_connection: SshConnectionHandle,
    event_tx: Sender<ForwardEvent>,
    detected_ports: Arc<Mutex<Vec<DetectedPort>>>,
    ignored_ports: Arc<Mutex<HashSet<u16>>>,
    lifecycle: Arc<AtomicU8>,
    mut stop_rx: oneshot::Receiver<()>,
) {
    let platform = match detect_remote_port_scan_platform(&ssh_connection).await {
        Some(platform) => platform,
        None => {
            lifecycle.store(ProfilerLifecycle::Degraded.as_u8(), Ordering::Release);
            return;
        }
    };
    let command = format!("{}\n", platform.scan_command());
    let init_command = match platform {
        RemotePortScanPlatform::Windows => PROFILER_INIT_WINDOWS,
        _ => PROFILER_INIT_UNIX,
    };
    let mut shell = match open_profiler_shell(&ssh_connection, init_command).await {
        Ok(shell) => shell,
        Err(error) => {
            tracing::warn!("failed to start port detection profiler {connection_id}: {error}");
            lifecycle.store(ProfilerLifecycle::Degraded.as_u8(), Ordering::Release);
            return;
        }
    };

    let mut previous_ports = HashSet::new();
    let mut initial_scan = true;
    let mut consecutive_failures = 0;
    let mut interval = tokio::time::interval(PROFILER_INTERVAL);
    interval.tick().await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                if !connection_is_profileable(&ssh_connection).await {
                    tracing::debug!("port detection profiler stopped for {connection_id}: SSH connection is no longer active");
                    break;
                }
                if consecutive_failures >= PROFILER_MAX_CONSECUTIVE_FAILURES {
                    lifecycle.store(ProfilerLifecycle::Degraded.as_u8(), Ordering::Release);
                    tracing::warn!("port detection profiler degraded for {connection_id} after {consecutive_failures} consecutive failures");
                    continue;
                }
                let output = match shell
                    .sample_until(&command, PROFILER_END_MARKER, PROFILER_SAMPLE_TIMEOUT, PORT_SCAN_MAX_OUTPUT_SIZE)
                    .await
                {
                    Ok(output) => {
                        consecutive_failures = 0;
                        lifecycle.store(ProfilerLifecycle::Running.as_u8(), Ordering::Release);
                        output
                    }
                    Err(error) => {
                        consecutive_failures += 1;
                        tracing::warn!(
                            "port detection profiler sample failed for {connection_id} ({consecutive_failures}/{PROFILER_MAX_CONSECUTIVE_FAILURES}): {error}"
                        );
                        if let Ok(new_shell) = open_profiler_shell(&ssh_connection, init_command)
                            .await
                        {
                            shell = new_shell;
                            tracing::debug!("port detection profiler reopened shell channel for {connection_id}");
                        }
                        continue;
                    }
                };
                if !output.contains(PROFILER_END_MARKER) {
                    tracing::warn!("port detection profiler sample for {connection_id} was truncated; skipping diff");
                    continue;
                }
                let current_ports = crate::detection::parse_listening_ports(&output, platform);
                let current_port_numbers = current_ports.iter().map(|port| port.port).collect::<HashSet<_>>();
                if initial_scan {
                    previous_ports = current_port_numbers;
                    *detected_ports.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = current_ports;
                    initial_scan = false;
                    continue;
                }

                let new_port_numbers = current_port_numbers
                    .difference(&previous_ports)
                    .copied()
                    .collect::<HashSet<_>>();
                let closed_port_numbers = previous_ports
                    .difference(&current_port_numbers)
                    .copied()
                    .collect::<HashSet<_>>();
                let ignored = ignored_ports
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone();
                let new_ports = current_ports
                    .iter()
                    .filter(|port| {
                        new_port_numbers.contains(&port.port)
                            && port.port != 22
                            && !ignored.contains(&port.port)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let closed_ports = closed_port_numbers
                    .iter()
                    .map(|port| DetectedPort {
                        port: *port,
                        bind_addr: String::new(),
                        process_name: None,
                        pid: None,
                    })
                    .collect::<Vec<_>>();
                *detected_ports.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = current_ports.clone();
                previous_ports = current_port_numbers;
                if !new_ports.is_empty() || !closed_ports.is_empty() {
                    let _ = event_tx.send(ForwardEvent::PortDetected {
                        connection_id: connection_id.clone(),
                        new_ports,
                        closed_ports,
                        all_ports: current_ports,
                    });
                }
            }
            _ = &mut stop_rx => {
                break;
            }
        }
    }
    let _ = shell.close().await;
    lifecycle.store(ProfilerLifecycle::Stopped.as_u8(), Ordering::Release);
}

async fn open_profiler_shell(
    ssh_connection: &SshConnectionHandle,
    init_command: &str,
) -> Result<oxideterm_ssh::SshShellChannel, oxideterm_ssh::SshTransportError> {
    tokio::time::timeout(
        PROFILER_CHANNEL_OPEN_TIMEOUT,
        ssh_connection.open_persistent_shell_channel(init_command),
    )
    .await
    .map_err(|_| oxideterm_ssh::SshTransportError::Timeout)?
}

async fn detect_remote_port_scan_platform(
    ssh_connection: &SshConnectionHandle,
) -> Option<RemotePortScanPlatform> {
    if !connection_is_profileable(ssh_connection).await {
        return None;
    }
    match ssh_connection
        .run_command(
            REMOTE_OS_PROBE_UNIX,
            Duration::from_secs(REMOTE_OS_PROBE_TIMEOUT_SECS),
            PORT_SCAN_MAX_OUTPUT_SIZE,
        )
        .await
    {
        Ok(output) => {
            let platform = crate::detection::classify_remote_platform(&output);
            if platform == RemotePortScanPlatform::Unknown {
                detect_windows_port_scan_platform(ssh_connection).await
            } else {
                Some(platform)
            }
        }
        Err(_) => detect_windows_port_scan_platform(ssh_connection).await,
    }
}

async fn detect_windows_port_scan_platform(
    ssh_connection: &SshConnectionHandle,
) -> Option<RemotePortScanPlatform> {
    match ssh_connection
        .run_command(
            REMOTE_OS_PROBE_WINDOWS,
            Duration::from_secs(REMOTE_OS_PROBE_TIMEOUT_SECS),
            PORT_SCAN_MAX_OUTPUT_SIZE,
        )
        .await
    {
        Ok(output) => {
            let platform = crate::detection::classify_remote_platform(&output);
            if platform == RemotePortScanPlatform::Windows {
                Some(RemotePortScanPlatform::Windows)
            } else {
                Some(RemotePortScanPlatform::Unknown)
            }
        }
        Err(_) => None,
    }
}

async fn connection_is_profileable(ssh_connection: &SshConnectionHandle) -> bool {
    if matches!(
        ssh_connection.state(),
        ConnectionState::LinkDown
            | ConnectionState::Reconnecting
            | ConnectionState::Disconnecting
            | ConnectionState::Disconnected
            | ConnectionState::Error(_)
    ) {
        return false;
    }

    matches!(
        ssh_connection.transport_status().await,
        ConnectionTransportStatus::Open
    )
}
