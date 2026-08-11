// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use fernomade_crypto::SessionKey;
use fernomade_runtime::{
    ConnectionState, MonotonicTime, RuntimeError, SessionAction, SessionRuntime, ShutdownOutcome,
    TerminalInputEvent,
};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::MoshIpFamily;

const COMMAND_CHANNEL_CAPACITY: usize = 256;
const EVENT_CHANNEL_CAPACITY: usize = 512;
const MAX_DATAGRAM_BYTES: usize = u16::MAX as usize;
const MAX_TIMER_SLEEP: Duration = Duration::from_secs(1);
const SUSPEND_GAP: Duration = Duration::from_secs(5);

pub struct MoshSessionConfig {
    pub remote_host: String,
    pub remote_port: u16,
    pub ip_family: MoshIpFamily,
    pub columns: u16,
    pub rows: u16,
    pub key: SessionKey,
}

impl fmt::Debug for MoshSessionConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MoshSessionConfig")
            .field("remote_host", &self.remote_host)
            .field("remote_port", &self.remote_port)
            .field("ip_family", &self.ip_family)
            .field("columns", &self.columns)
            .field("rows", &self.rows)
            .field("key", &"[REDACTED]")
            .finish()
    }
}

enum MoshSessionCommand {
    Input { prediction_id: u64, bytes: Vec<u8> },
    Resize { columns: u16, rows: u16 },
    Shutdown,
    Cancel,
}

impl fmt::Debug for MoshSessionCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input {
                prediction_id,
                bytes,
            } => formatter
                .debug_struct("Input")
                .field("prediction_id", prediction_id)
                .field("bytes", &bytes.len())
                .finish(),
            Self::Resize { columns, rows } => formatter
                .debug_struct("Resize")
                .field("columns", columns)
                .field("rows", rows)
                .finish(),
            Self::Shutdown => formatter.write_str("Shutdown"),
            Self::Cancel => formatter.write_str("Cancel"),
        }
    }
}

pub enum MoshSessionEvent {
    Output(Vec<u8>),
    RemoteResize { columns: u16, rows: u16 },
    ConnectionStateChanged(ConnectionState),
    RoundTripEstimate(u16),
    PredictionAcknowledged(u64),
    RemoteStateAdvanced(u64),
    Closed(ShutdownOutcome),
    Failed(String),
}

impl fmt::Debug for MoshSessionEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Output(bytes) => formatter
                .debug_struct("Output")
                .field("bytes", &bytes.len())
                .finish(),
            Self::RemoteResize { columns, rows } => formatter
                .debug_struct("RemoteResize")
                .field("columns", columns)
                .field("rows", rows)
                .finish(),
            Self::ConnectionStateChanged(state) => formatter
                .debug_tuple("ConnectionStateChanged")
                .field(state)
                .finish(),
            Self::RoundTripEstimate(milliseconds) => formatter
                .debug_tuple("RoundTripEstimate")
                .field(milliseconds)
                .finish(),
            Self::PredictionAcknowledged(state_id) => formatter
                .debug_tuple("PredictionAcknowledged")
                .field(state_id)
                .finish(),
            Self::RemoteStateAdvanced(state_id) => formatter
                .debug_tuple("RemoteStateAdvanced")
                .field(state_id)
                .finish(),
            Self::Closed(outcome) => formatter.debug_tuple("Closed").field(outcome).finish(),
            Self::Failed(message) => formatter.debug_tuple("Failed").field(message).finish(),
        }
    }
}

pub struct MoshSessionClient {
    command_tx: mpsc::Sender<MoshSessionCommand>,
    event_rx: mpsc::Receiver<MoshSessionEvent>,
}

impl MoshSessionClient {
    pub async fn send_input_for_prediction(
        &self,
        prediction_id: u64,
        bytes: Vec<u8>,
    ) -> Result<(), MoshSessionCommandError> {
        self.command_tx
            .send(MoshSessionCommand::Input {
                prediction_id,
                bytes,
            })
            .await
            .map_err(|_| MoshSessionCommandError::Closed)
    }

    pub async fn resize(&self, columns: u16, rows: u16) -> Result<(), MoshSessionCommandError> {
        self.command_tx
            .send(MoshSessionCommand::Resize { columns, rows })
            .await
            .map_err(|_| MoshSessionCommandError::Closed)
    }

    pub async fn next_event(&mut self) -> Option<MoshSessionEvent> {
        self.event_rx.recv().await
    }

    pub fn try_next_event(&mut self) -> Option<MoshSessionEvent> {
        self.event_rx.try_recv().ok()
    }
}

pub struct MoshSessionOwner {
    command_tx: mpsc::Sender<MoshSessionCommand>,
    task: Option<JoinHandle<()>>,
}

impl MoshSessionOwner {
    pub async fn shutdown(mut self) {
        let _ = self.command_tx.send(MoshSessionCommand::Shutdown).await;
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }

    pub fn cancel(&mut self) {
        let _ = self.command_tx.try_send(MoshSessionCommand::Cancel);
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl Drop for MoshSessionOwner {
    fn drop(&mut self) {
        // Abrupt owner loss must drop protocol key material immediately.
        self.cancel();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MoshSessionStartError {
    #[error("Mosh UDP endpoint is invalid")]
    InvalidEndpoint,
    #[error("Mosh UDP endpoint could not be resolved")]
    EndpointResolutionFailed,
    #[error("Mosh UDP socket could not be opened")]
    SocketOpenFailed,
}

#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
pub enum MoshSessionCommandError {
    #[error("Mosh session is closed")]
    Closed,
}

pub async fn start_mosh_session(
    config: MoshSessionConfig,
) -> Result<(MoshSessionClient, MoshSessionOwner), MoshSessionStartError> {
    if config.remote_port == 0 || config.columns == 0 || config.rows == 0 {
        return Err(MoshSessionStartError::InvalidEndpoint);
    }
    let remote_address =
        resolve_remote_address(&config.remote_host, config.remote_port, config.ip_family).await?;
    let bind_address = match remote_address.ip() {
        IpAddr::V4(_) => "0.0.0.0:0",
        IpAddr::V6(_) => "[::]:0",
    };
    let socket = UdpSocket::bind(bind_address)
        .await
        .map_err(|_| MoshSessionStartError::SocketOpenFailed)?;
    socket
        .connect(remote_address)
        .await
        .map_err(|_| MoshSessionStartError::SocketOpenFailed)?;

    let (command_tx, command_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
    let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
    let owner_command_tx = command_tx.clone();
    let task = tokio::spawn(run_session(
        socket,
        config.key,
        config.columns,
        config.rows,
        command_rx,
        event_tx,
    ));
    Ok((
        MoshSessionClient {
            command_tx,
            event_rx,
        },
        MoshSessionOwner {
            command_tx: owner_command_tx,
            task: Some(task),
        },
    ))
}

async fn resolve_remote_address(
    host: &str,
    port: u16,
    family: MoshIpFamily,
) -> Result<SocketAddr, MoshSessionStartError> {
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| MoshSessionStartError::EndpointResolutionFailed)?;
    addresses
        .into_iter()
        .find(|address| match family {
            MoshIpFamily::Auto => true,
            MoshIpFamily::Ipv4 => address.is_ipv4(),
            MoshIpFamily::Ipv6 => address.is_ipv6(),
        })
        .ok_or(MoshSessionStartError::EndpointResolutionFailed)
}

async fn run_session(
    socket: UdpSocket,
    key: SessionKey,
    columns: u16,
    rows: u16,
    mut command_rx: mpsc::Receiver<MoshSessionCommand>,
    event_tx: mpsc::Sender<MoshSessionEvent>,
) {
    let started_at = Instant::now();
    let mut previous_loop = started_at;
    let mut runtime = SessionRuntime::new(key, monotonic_time(started_at));
    runtime.queue_resize(columns, rows);
    let mut prediction_ids = PredictionIdMap::default();
    let mut receive_buffer = vec![0_u8; MAX_DATAGRAM_BYTES];

    loop {
        let now = Instant::now();
        if now.duration_since(previous_loop) > SUSPEND_GAP {
            runtime.resume(monotonic_time(started_at));
        }
        previous_loop = now;
        let actions = match runtime.poll(monotonic_time(started_at)) {
            Ok(actions) => actions,
            Err(error) => {
                let _ = event_tx
                    .send(MoshSessionEvent::Failed(error.to_string()))
                    .await;
                let _ = runtime.cancel();
                return;
            }
        };
        if !apply_actions(&socket, &event_tx, &mut prediction_ids, actions).await {
            let _ = runtime.cancel();
            return;
        }
        if runtime.shutdown_outcome().is_some() {
            return;
        }

        let poll_wait = runtime
            .milliseconds_until_next_poll(monotonic_time(started_at))
            .min(MAX_TIMER_SLEEP.as_millis() as u64);
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(MoshSessionCommand::Input { prediction_id, bytes }) => {
                        if let Err(error) = queue_prediction_input(
                            &mut runtime,
                            &mut prediction_ids,
                            prediction_id,
                            bytes,
                        ) {
                            let _ = event_tx.send(MoshSessionEvent::Failed(error.to_string())).await;
                            let _ = runtime.cancel();
                            return;
                        }
                    }
                    Some(MoshSessionCommand::Resize { columns, rows }) => {
                        if columns > 0 && rows > 0 {
                            let _ = runtime.queue_terminal_event(TerminalInputEvent::Resize { columns, rows });
                        }
                    }
                    Some(MoshSessionCommand::Shutdown) => {
                        match runtime.request_shutdown(monotonic_time(started_at)) {
                            Ok(actions) => {
                                if !apply_actions(
                                    &socket,
                                    &event_tx,
                                    &mut prediction_ids,
                                    actions,
                                ).await {
                                    let _ = runtime.cancel();
                                    return;
                                }
                            }
                            Err(error) => {
                                let _ = event_tx.send(MoshSessionEvent::Failed(error.to_string())).await;
                                let _ = runtime.cancel();
                                return;
                            }
                        }
                    }
                    Some(MoshSessionCommand::Cancel) | None => {
                        let _ = runtime.cancel();
                        return;
                    }
                }
            }
            received = socket.recv(&mut receive_buffer) => {
                match received {
                    Ok(length) => {
                        let actions = runtime.receive_datagram_lossy(
                            &receive_buffer[..length],
                            monotonic_time(started_at),
                        );
                        if !apply_actions(
                            &socket,
                            &event_tx,
                            &mut prediction_ids,
                            actions,
                        ).await {
                            let _ = runtime.cancel();
                            return;
                        }
                    }
                    Err(_) => {
                        let _ = event_tx.send(MoshSessionEvent::Failed(
                            "Mosh UDP receive failed".to_string(),
                        )).await;
                        let _ = runtime.cancel();
                        return;
                    }
                }
            }
            () = tokio::time::sleep(Duration::from_millis(poll_wait)) => {}
        }
    }
}

async fn apply_actions(
    socket: &UdpSocket,
    event_tx: &mpsc::Sender<MoshSessionEvent>,
    prediction_ids: &mut PredictionIdMap,
    actions: Vec<SessionAction>,
) -> bool {
    for action in actions {
        let event = match action {
            SessionAction::SendDatagram(datagram) => {
                if socket.send(&datagram).await.is_err() {
                    let _ = event_tx
                        .send(MoshSessionEvent::Failed("Mosh UDP send failed".to_string()))
                        .await;
                    return false;
                }
                continue;
            }
            SessionAction::WriteTerminal(bytes) => MoshSessionEvent::Output(bytes),
            SessionAction::ResizeTerminal { columns, rows } => {
                MoshSessionEvent::RemoteResize { columns, rows }
            }
            SessionAction::AcknowledgePrediction(protocol_frame_id) => {
                let Some(prediction_id) = prediction_ids.acknowledge(protocol_frame_id) else {
                    continue;
                };
                MoshSessionEvent::PredictionAcknowledged(prediction_id)
            }
            SessionAction::RemoteStateAdvanced(state_id) => {
                MoshSessionEvent::RemoteStateAdvanced(state_id)
            }
            SessionAction::ConnectionStateChanged(state) => {
                MoshSessionEvent::ConnectionStateChanged(state)
            }
            SessionAction::RoundTripEstimate(milliseconds) => {
                MoshSessionEvent::RoundTripEstimate(milliseconds)
            }
            SessionAction::ShutdownComplete(outcome) => MoshSessionEvent::Closed(outcome),
            SessionAction::CapabilitiesChanged(_)
            | SessionAction::RemoteSessionControl { .. }
            | SessionAction::SessionLifecycleChanged(_)
            | SessionAction::UdpBindingChanged(_)
            | SessionAction::Diagnostic(_) => continue,
        };
        if event_tx.send(event).await.is_err() {
            return false;
        }
    }
    true
}

#[derive(Default)]
struct PredictionIdMap {
    by_protocol_frame: BTreeMap<u64, u64>,
}

impl PredictionIdMap {
    fn record(&mut self, protocol_frame_id: u64, prediction_id: u64) {
        self.by_protocol_frame
            .entry(protocol_frame_id)
            .and_modify(|current| *current = (*current).max(prediction_id))
            .or_insert(prediction_id);
    }

    fn acknowledge(&mut self, protocol_frame_id: u64) -> Option<u64> {
        let prediction_id = self
            .by_protocol_frame
            .range(..=protocol_frame_id)
            .next_back()
            .map(|(_, prediction_id)| *prediction_id);
        self.by_protocol_frame
            .retain(|frame_id, _| *frame_id > protocol_frame_id);
        prediction_id
    }
}

fn queue_prediction_input(
    runtime: &mut SessionRuntime,
    prediction_ids: &mut PredictionIdMap,
    prediction_id: u64,
    bytes: Vec<u8>,
) -> Result<(), RuntimeError> {
    // The runtime alone owns SSP numbering; bridge its frame back to the terminal-local prediction.
    let protocol_frame_id = runtime.prediction_frame_id();
    runtime.queue_terminal_event(TerminalInputEvent::Bytes(bytes))?;
    prediction_ids.record(protocol_frame_id, prediction_id);
    Ok(())
}

fn monotonic_time(started_at: Instant) -> MonotonicTime {
    let elapsed = Instant::now().duration_since(started_at).as_millis();
    MonotonicTime::from_milliseconds(u64::try_from(elapsed).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNTHETIC_KEY: &str = "AQIDBAUGBwgJCgsMDQ4PEA==";

    #[test]
    fn terminal_prediction_ids_do_not_depend_on_protocol_state_numbers() {
        let key = SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode");
        let mut runtime = SessionRuntime::new(key, MonotonicTime::from_milliseconds(0));
        runtime.queue_resize(80, 24);
        runtime
            .poll(MonotonicTime::from_milliseconds(0))
            .expect("initial protocol state must open");
        let protocol_frame_id = runtime.prediction_frame_id();
        assert_ne!(protocol_frame_id, 0);

        let mut prediction_ids = PredictionIdMap::default();
        queue_prediction_input(
            &mut runtime,
            &mut prediction_ids,
            0,
            b"first input".to_vec(),
        )
        .expect("terminal-local prediction zero must be accepted");

        assert_eq!(prediction_ids.acknowledge(protocol_frame_id), Some(0));
    }

    #[test]
    fn prediction_acknowledgements_coalesce_by_protocol_frame() {
        let mut prediction_ids = PredictionIdMap::default();
        prediction_ids.record(4, 0);
        prediction_ids.record(4, 1);
        prediction_ids.record(7, 2);

        assert_eq!(prediction_ids.acknowledge(4), Some(1));
        assert_eq!(prediction_ids.acknowledge(6), None);
        assert_eq!(prediction_ids.acknowledge(7), Some(2));
    }
}
