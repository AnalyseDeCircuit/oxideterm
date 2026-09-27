// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fernomade_crypto::SessionKey;
use fernomade_runtime::{
    ConnectionState, MonotonicTime, RuntimeError, SessionAction, SessionRuntime, ShutdownOutcome,
    TerminalInputEvent,
};
use tokio::net::UdpSocket;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio::task::JoinHandle;
use zeroize::Zeroize;

use crate::MoshIpFamily;

const COMMAND_CHANNEL_CAPACITY: usize = 256;
const EVENT_CHANNEL_CAPACITY: usize = 512;
const EVENT_OUTPUT_BYTES: usize = 1024 * 1024;
const OUTPUT_CHUNK_BYTES: usize = 8 * 1024;
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

// The byte permit follows queued output until the bridge takes ownership.
// Pending protocol actions and cancelled events wipe their owned plaintext.
struct QueuedEvent {
    event: MoshSessionEvent,
    permit: Option<OwnedSemaphorePermit>,
}
impl QueuedEvent {
    fn new(event: MoshSessionEvent) -> Self {
        Self {
            event,
            permit: None,
        }
    }
    fn into_event(mut self) -> MoshSessionEvent {
        std::mem::replace(&mut self.event, MoshSessionEvent::RemoteStateAdvanced(0))
    }
    fn bytes(&self) -> usize {
        match &self.event {
            MoshSessionEvent::Output(bytes) => bytes.len(),
            _ => 0,
        }
    }
}
impl Drop for QueuedEvent {
    fn drop(&mut self) {
        if let MoshSessionEvent::Output(bytes) = &mut self.event {
            bytes.zeroize();
        }
    }
}

pub struct MoshSessionClient {
    command_tx: mpsc::Sender<MoshSessionCommand>,
    event_rx: mpsc::Receiver<QueuedEvent>,
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
        self.event_rx.recv().await.map(QueuedEvent::into_event)
    }

    pub fn try_next_event(&mut self) -> Option<MoshSessionEvent> {
        self.event_rx.try_recv().ok().map(QueuedEvent::into_event)
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
    event_tx: mpsc::Sender<QueuedEvent>,
) {
    let started_at = Instant::now();
    let mut previous_loop = started_at;
    let mut runtime = SessionRuntime::new(key, monotonic_time(started_at));
    runtime.queue_resize(columns, rows);
    let mut prediction_ids = PredictionIdMap::default();
    let mut receive_buffer = vec![0_u8; MAX_DATAGRAM_BYTES];
    let bytes = Arc::new(Semaphore::new(EVENT_OUTPUT_BYTES));
    let mut pending = VecDeque::new();
    let mut ending = false;

    loop {
        if !ending {
            let now = Instant::now();
            if now.duration_since(previous_loop) > SUSPEND_GAP {
                runtime.resume(monotonic_time(started_at));
            }
            previous_loop = now;
            match runtime.poll(monotonic_time(started_at)) {
                Ok(actions) => {
                    ending =
                        !apply_actions(&socket, &mut pending, &mut prediction_ids, actions).await;
                }
                Err(error) => {
                    pending.push_back(QueuedEvent::new(MoshSessionEvent::Failed(
                        error.to_string(),
                    )));
                    ending = true;
                }
            }
            ending |= runtime.shutdown_outcome().is_some();
        }
        if ending {
            command_rx.close();
        }
        if ending && pending.is_empty() {
            return;
        }
        let poll_wait = runtime
            .milliseconds_until_next_poll(monotonic_time(started_at))
            .min(MAX_TIMER_SLEEP.as_millis() as u64);
        let next_bytes = pending.front().map_or(0, QueuedEvent::bytes) as u32;
        tokio::select! {
            capacity = async {
                let permit = bytes.clone().acquire_many_owned(next_bytes).await.expect("live output budget");
                event_tx.reserve().await.map(|slot| (permit, slot))
            }, if !pending.is_empty() => {
                let Ok((permit, slot)) = capacity else { let _ = runtime.cancel(); return; };
                let mut event = pending.pop_front().unwrap();
                event.permit = Some(permit);
                slot.send(event);
            }
            command = command_rx.recv(), if !ending => {
                let result = match command {
                    Some(MoshSessionCommand::Input { prediction_id, bytes }) => {
                        queue_prediction_input(&mut runtime, &mut prediction_ids, prediction_id, bytes)
                            .map(|_| Vec::new())
                    }
                    Some(MoshSessionCommand::Resize { columns, rows }) => {
                        if columns > 0 && rows > 0 { runtime.queue_resize(columns, rows); }
                        Ok(Vec::new())
                    }
                    Some(MoshSessionCommand::Shutdown) => runtime.request_shutdown(monotonic_time(started_at)),
                    Some(MoshSessionCommand::Cancel) | None => { let _ = runtime.cancel(); return; }
                };
                match result {
                    Ok(actions) => {
                        ending = !apply_actions(&socket, &mut pending, &mut prediction_ids, actions).await;
                    }
                    Err(error) => {
                        pending.push_back(QueuedEvent::new(MoshSessionEvent::Failed(error.to_string())));
                        ending = true;
                    }
                }
            }
            // Do not accept/acknowledge another SSP update while the preceding
            // display actions are waiting. Timers and outgoing input stay live;
            // UDP loss during this pause is recovered by the protocol's retry.
            received = socket.recv(&mut receive_buffer), if pending.is_empty() && !ending => {
                match received {
                    Ok(length) => {
                        let actions = runtime.receive_datagram_lossy(&receive_buffer[..length], monotonic_time(started_at));
                        ending = !apply_actions(&socket, &mut pending, &mut prediction_ids, actions).await;
                    }
                    Err(_) => {
                        pending.push_back(QueuedEvent::new(MoshSessionEvent::Failed("Mosh UDP receive failed".into())));
                        ending = true;
                    }
                }
            }
            () = tokio::time::sleep(Duration::from_millis(poll_wait)), if !ending => {}
            () = event_tx.closed() => { let _ = runtime.cancel(); return; }
        }
    }
}

async fn apply_actions(
    socket: &UdpSocket,
    pending: &mut VecDeque<QueuedEvent>,
    prediction_ids: &mut PredictionIdMap,
    actions: Vec<SessionAction>,
) -> bool {
    for action in actions {
        let event = match action {
            SessionAction::SendDatagram(datagram) => {
                if socket.send(&datagram).await.is_err() {
                    pending.push_back(QueuedEvent::new(MoshSessionEvent::Failed(
                        "Mosh UDP send failed".into(),
                    )));
                    return false;
                }
                continue;
            }
            SessionAction::WriteTerminal(bytes) => {
                let bytes = zeroize::Zeroizing::new(bytes);
                for chunk in bytes.chunks(OUTPUT_CHUNK_BYTES) {
                    pending.push_back(QueuedEvent::new(MoshSessionEvent::Output(chunk.to_vec())));
                }
                continue;
            }
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
        pending.push_back(QueuedEvent::new(event));
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

    #[tokio::test]
    async fn stalled_display_keeps_udp_input_timers_and_cancel_live() {
        for cancel in [false, true] {
            run_stalled_display(cancel).await;
        }
    }

    async fn run_stalled_display(cancel: bool) {
        use fernomade_crypto::{PeerRole, SecureChannel};
        use fernomade_wire::{
            ByteRun, Fragment, Instruction, InstructionBatch, StateUpdate, ViewportSize,
            decode_compressed_update, encode_compressed_update,
        };
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        socket.connect(server.local_addr().unwrap()).await.unwrap();
        let (commands, command_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
        let (events, mut receiver) = mpsc::channel(1);
        // Leave a preceding event unread so the real delivery mailbox is full.
        events
            .send(QueuedEvent::new(MoshSessionEvent::RemoteStateAdvanced(999)))
            .await
            .unwrap();
        let mut task = tokio::spawn(run_session(
            socket,
            SessionKey::decode(SYNTHETIC_KEY).unwrap(),
            80,
            24,
            command_rx,
            events,
        ));
        let mut secure =
            SecureChannel::new(PeerRole::Server, SessionKey::decode(SYNTHETIC_KEY).unwrap());
        let mut buffer = vec![0; MAX_DATAGRAM_BYTES];
        let mut initial = 0;
        while initial < 1 {
            let (length, _) =
                tokio::time::timeout(Duration::from_secs(3), server.recv_from(&mut buffer))
                    .await
                    .unwrap()
                    .unwrap();
            let packet = secure.open(&buffer[..length]).unwrap();
            let fragment = Fragment::parse(&packet.plaintext).unwrap();
            initial = decode_compressed_update(&fragment.body)
                .unwrap()
                .target_state;
        }
        let mut update = StateUpdate::new(0, 1, initial);
        update.delta = InstructionBatch {
            instructions: vec![Instruction {
                bytes: Some(ByteRun {
                    value: b"remote display".to_vec(),
                }),
                viewport: Some(ViewportSize {
                    columns: 100,
                    rows: 30,
                }),
                marker: None,
                session_control: None,
            }],
        }
        .encode_bytes();
        for fragment in
            Fragment::split(&encode_compressed_update(&update).unwrap(), 1, 0, 1).unwrap()
        {
            server
                .send_to(&secure.seal_next(&fragment.encode()).unwrap(), address)
                .await
                .unwrap();
        }
        // Acknowledgement comes from the protocol timer after accepting the
        // update; the display receiver remains full throughout this exchange.
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let (length, _) = server.recv_from(&mut buffer).await.unwrap();
                let packet = secure.open(&buffer[..length]).unwrap();
                let fragment = Fragment::parse(&packet.plaintext).unwrap();
                if decode_compressed_update(&fragment.body)
                    .unwrap()
                    .acknowledged_state
                    == 1
                {
                    break;
                }
            }
        })
        .await
        .expect("display queue blocked the SSP acknowledgement timer");
        commands
            .send(MoshSessionCommand::Input {
                prediction_id: 42,
                bytes: b"control-under-pressure".to_vec(),
            })
            .await
            .unwrap();
        let input = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let (length, _) = server.recv_from(&mut buffer).await.unwrap();
                let packet = secure.open(&buffer[..length]).unwrap();
                let fragment = Fragment::parse(&packet.plaintext).unwrap();
                let update = decode_compressed_update(&fragment.body).unwrap();
                let input = update
                    .decode_instructions()
                    .unwrap()
                    .instructions
                    .into_iter()
                    .filter_map(|instruction| instruction.bytes)
                    .flat_map(|bytes| bytes.value)
                    .collect::<Vec<_>>();
                if !input.is_empty() {
                    break input;
                }
            }
        })
        .await
        .expect("display queue blocked UDP input");
        assert_eq!(input, b"control-under-pressure");
        if cancel {
            commands.send(MoshSessionCommand::Cancel).await.unwrap();
            tokio::time::timeout(Duration::from_secs(3), &mut task)
                .await
                .expect("full display queue blocked cancellation")
                .unwrap();
            return;
        }

        assert!(matches!(
            receiver.recv().await.unwrap().into_event(),
            MoshSessionEvent::RemoteStateAdvanced(999)
        ));
        let (output, sizes) = tokio::time::timeout(Duration::from_secs(3), async {
            let mut output = Vec::new();
            let mut sizes = Vec::new();
            while sizes.is_empty() {
                match receiver.recv().await.unwrap().into_event() {
                    MoshSessionEvent::Output(bytes) => output.extend(bytes),
                    MoshSessionEvent::RemoteResize { columns, rows } => {
                        sizes.push((output.len(), columns, rows))
                    }
                    _ => {}
                }
            }
            (output, sizes)
        })
        .await
        .expect("display output did not resume");
        assert_eq!(output, b"remote display");
        assert_eq!(sizes, vec![(b"remote display".len(), 100, 30)]);
        commands.send(MoshSessionCommand::Cancel).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), &mut task)
            .await
            .expect("protocol task did not cancel")
            .unwrap();
    }

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
