// SPDX-License-Identifier: Apache-2.0

//! Deterministic host-facing session runtime without socket or terminal access.

use fernomade_crypto::SessionKey;
use fernomade_session::{ClientProtocol, ReceivedState, SessionError};
use fernomade_wire::{
    ByteRun, Instruction, InstructionBatch, MessageError, SESSION_CONTROL_CAPABILITY,
    SESSION_CREATE_REQUEST, SESSION_CREATED, SESSION_LIST_REQUEST, SESSION_LIST_RESPONSE,
    SESSION_SWITCH_REQUEST, SESSION_SWITCHED, SessionControl, ViewportSize,
};
use std::collections::VecDeque;
use std::fmt;

const INITIAL_RETRANSMIT_MILLISECONDS: u64 = 1_000;
const MINIMUM_RETRANSMIT_MILLISECONDS: u64 = 250;
const MAXIMUM_RETRANSMIT_MILLISECONDS: u64 = 10_000;
const SERVER_UNRESPONSIVE_MILLISECONDS: u64 = 30_000;
const MAXIMUM_QUEUED_INPUT_BYTES: usize = 1024 * 1024;
const SHUTDOWN_RETRY_LIMIT: u8 = 16;
const SHUTDOWN_TIMEOUT_MILLISECONDS: u64 = 10_000;
const MOSH_GO_NO_ECHO_TIMESTAMP: u16 = 0;
const STANDARD_MOSH_NO_ECHO_TIMESTAMP: u16 = u16::MAX;

/// Version of the host-facing runtime contract documented by this crate.
pub const EMBEDDING_API_VERSION: u16 = 10;

/// A monotonic millisecond value supplied by the embedding host.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MonotonicTime(u64);

impl MonotonicTime {
    #[must_use]
    pub const fn from_milliseconds(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn milliseconds(self) -> u64 {
        self.0
    }

    fn elapsed_since(self, earlier: Self) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

/// An effect the host must apply in order.
#[derive(Eq, PartialEq)]
pub enum SessionAction {
    SendDatagram(Vec<u8>),
    WriteTerminal(Vec<u8>),
    ResizeTerminal {
        columns: u16,
        rows: u16,
    },
    AcknowledgePrediction(u64),
    RemoteStateAdvanced(u64),
    ConnectionStateChanged(ConnectionState),
    RoundTripEstimate(u16),
    CapabilitiesChanged(Vec<u8>),
    RemoteSessionControl {
        kind: u32,
        payload: Vec<u8>,
    },
    SessionLifecycleChanged(SessionLifecycle),
    ShutdownComplete(ShutdownOutcome),
    UdpBindingChanged(u64),
    /// Reports content-free protocol metadata to an embedding host.
    Diagnostic(DiagnosticEvent),
}

impl fmt::Debug for SessionAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SendDatagram(bytes) => formatter
                .debug_struct("SendDatagram")
                .field("bytes", &bytes.len())
                .finish(),
            Self::WriteTerminal(bytes) => formatter
                .debug_struct("WriteTerminal")
                .field("bytes", &bytes.len())
                .finish(),
            Self::ResizeTerminal { columns, rows } => formatter
                .debug_struct("ResizeTerminal")
                .field("columns", columns)
                .field("rows", rows)
                .finish(),
            Self::AcknowledgePrediction(value) => formatter
                .debug_tuple("AcknowledgePrediction")
                .field(value)
                .finish(),
            Self::RemoteStateAdvanced(value) => formatter
                .debug_tuple("RemoteStateAdvanced")
                .field(value)
                .finish(),
            Self::ConnectionStateChanged(value) => formatter
                .debug_tuple("ConnectionStateChanged")
                .field(value)
                .finish(),
            Self::RoundTripEstimate(value) => formatter
                .debug_tuple("RoundTripEstimate")
                .field(value)
                .finish(),
            Self::CapabilitiesChanged(value) => formatter
                .debug_struct("CapabilitiesChanged")
                .field("bytes", &value.len())
                .finish(),
            Self::RemoteSessionControl { kind, payload } => formatter
                .debug_struct("RemoteSessionControl")
                .field("kind", kind)
                .field("payload_bytes", &payload.len())
                .finish(),
            Self::SessionLifecycleChanged(value) => formatter
                .debug_tuple("SessionLifecycleChanged")
                .field(value)
                .finish(),
            Self::ShutdownComplete(value) => formatter
                .debug_tuple("ShutdownComplete")
                .field(value)
                .finish(),
            Self::UdpBindingChanged(value) => formatter
                .debug_tuple("UdpBindingChanged")
                .field(value)
                .finish(),
            Self::Diagnostic(value) => formatter.debug_tuple("Diagnostic").field(value).finish(),
        }
    }
}

/// Content-free protocol metadata suitable for host-controlled diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticEvent {
    ConnectionStateChanged {
        state: ConnectionState,
    },
    FreshUpdatePrepared {
        state_id: u64,
        datagram_count: u64,
        datagram_bytes: u64,
        instruction_count: u64,
        input_bytes: u64,
    },
    RetransmissionPrepared {
        state_id: u64,
        datagram_count: u64,
        datagram_bytes: u64,
        retransmit_delay_milliseconds: u64,
    },
    InboundUpdateAccepted {
        packet_counter: u64,
        base_state: u64,
        target_state: u64,
        acknowledged_state: u64,
        discard_before: u64,
        delta_bytes: u64,
        advances_remote_state: bool,
    },
    RoundTripUpdated {
        milliseconds: u16,
    },
    SessionLifecycleChanged {
        state: SessionLifecycle,
    },
    UdpBindingChanged {
        generation: u64,
    },
    ShutdownStarted,
    ShutdownComplete {
        outcome: ShutdownOutcome,
    },
}

/// Describes why the bounded SSP shutdown handshake ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownOutcome {
    Acknowledged,
    PeerRequested,
    TimedOut,
}

/// Describes whether an embedding host is actively driving the session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SessionLifecycle {
    #[default]
    Running,
    Paused,
    Cancelled,
}

/// A terminal input event supplied by an embedding host without using stdio.
#[derive(Eq, PartialEq)]
pub enum TerminalInputEvent {
    Bytes(Vec<u8>),
    Resize { columns: u16, rows: u16 },
}

impl fmt::Debug for TerminalInputEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bytes(bytes) => formatter
                .debug_struct("Bytes")
                .field("length", &bytes.len())
                .finish(),
            Self::Resize { columns, rows } => formatter
                .debug_struct("Resize")
                .field("columns", columns)
                .field("rows", rows)
                .finish(),
        }
    }
}

/// Describes whether an authenticated server session has made recent progress.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConnectionState {
    /// No authenticated server state has been received yet.
    #[default]
    Connecting,
    /// Authenticated server traffic is arriving normally.
    Connected,
    /// A previously connected session has stopped responding temporarily.
    Interrupted,
}

/// A read-only snapshot of content-free SSP transport state for an embedding host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportStatus {
    pub acked_by_remote: u64,
    pub sent_num: u64,
    pub last_recv_old_num: u64,
    pub last_recv_new_num: u64,
    pub throwaway_num: u64,
    pub retransmit_timeout_milliseconds: u64,
}

/// A raw SSP update for hosts that own terminal-state reconstruction.
#[derive(Eq, PartialEq)]
pub struct RawRemoteUpdate {
    pub old_num: u64,
    pub new_num: u64,
    pub ack_num: u64,
    pub throwaway_num: u64,
    pub diff: Vec<u8>,
}

impl fmt::Debug for RawRemoteUpdate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RawRemoteUpdate")
            .field("old_num", &self.old_num)
            .field("new_num", &self.new_num)
            .field("ack_num", &self.ack_num)
            .field("throwaway_num", &self.throwaway_num)
            .field("diff_bytes", &self.diff.len())
            .finish()
    }
}

/// Owns synchronization, timers, and bounded local input pending transport.
pub struct SessionRuntime {
    protocol: ClientProtocol,
    queued_instructions: VecDeque<Instruction>,
    queued_input_bytes: usize,
    last_send: Option<MonotonicTime>,
    last_receive: MonotonicTime,
    retransmit_milliseconds: u64,
    smoothed_round_trip_milliseconds: Option<u64>,
    round_trip_variation_milliseconds: u64,
    association: AssociationState,
    acknowledgement_due: bool,
    received_server_state: bool,
    round_trip_milliseconds: Option<u16>,
    connection_state: ConnectionState,
    lifecycle: SessionLifecycle,
    udp_binding_generation: u64,
    shutdown: ShutdownState,
    peer_shutdown_acknowledgement_due: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShutdownState {
    Open,
    LocalRequested {
        started_at: MonotonicTime,
        transmissions: u8,
    },
    PeerRequested,
    Complete(ShutdownOutcome),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AssociationState {
    Due,
    Sent,
}

struct ProcessedDatagram {
    actions: Vec<SessionAction>,
    raw_update: Option<RawRemoteUpdate>,
}

impl SessionRuntime {
    #[must_use]
    pub fn new(key: SessionKey, now: MonotonicTime) -> Self {
        Self {
            protocol: ClientProtocol::new(key),
            queued_instructions: VecDeque::new(),
            queued_input_bytes: 0,
            last_send: None,
            last_receive: now,
            retransmit_milliseconds: INITIAL_RETRANSMIT_MILLISECONDS,
            smoothed_round_trip_milliseconds: None,
            round_trip_variation_milliseconds: 0,
            association: AssociationState::Due,
            acknowledgement_due: false,
            received_server_state: false,
            round_trip_milliseconds: None,
            connection_state: ConnectionState::Connecting,
            lifecycle: SessionLifecycle::Running,
            udp_binding_generation: 0,
            shutdown: ShutdownState::Open,
            peer_shutdown_acknowledgement_due: false,
        }
    }

    /// Queues a host-provided terminal event without reading process stdio.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is paused, cancelled, shutting down, or
    /// the input queue would exceed its fixed memory bound.
    pub fn queue_terminal_event(&mut self, event: TerminalInputEvent) -> Result<(), RuntimeError> {
        self.ensure_accepting_input()?;
        match event {
            TerminalInputEvent::Bytes(bytes) => self.queue_input(bytes),
            TerminalInputEvent::Resize { columns, rows } => {
                self.queue_resize(columns, rows);
                Ok(())
            }
        }
    }

    /// Queues terminal input under a fixed memory bound.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime cannot accept input or if total unsent
    /// input would exceed the bound.
    pub fn queue_input(&mut self, bytes: Vec<u8>) -> Result<(), RuntimeError> {
        self.ensure_accepting_input()?;
        let new_size = self
            .queued_input_bytes
            .checked_add(bytes.len())
            .ok_or(RuntimeError::InputQueueFull)?;
        if new_size > MAXIMUM_QUEUED_INPUT_BYTES {
            return Err(RuntimeError::InputQueueFull);
        }
        self.queued_input_bytes = new_size;
        self.queued_instructions.push_back(Instruction {
            bytes: Some(ByteRun { value: bytes }),
            viewport: None,
            marker: None,
            session_control: None,
        });
        Ok(())
    }

    pub fn queue_resize(&mut self, columns: u16, rows: u16) {
        if self.lifecycle != SessionLifecycle::Running || self.shutdown != ShutdownState::Open {
            return;
        }
        self.protocol.set_initial_remote_viewport(columns, rows);
        let viewport = ViewportSize {
            columns: u64::from(columns),
            rows: u64::from(rows),
        };
        self.queued_instructions.push_back(Instruction {
            bytes: None,
            viewport: Some(viewport),
            marker: None,
            session_control: None,
        });
    }

    /// Sets capability bytes advertised on subsequent protocol updates.
    pub fn set_local_capabilities(&mut self, capabilities: Vec<u8>) {
        self.protocol.set_local_capabilities(capabilities);
    }

    /// Returns the latest non-empty capability advertisement received from the peer.
    #[must_use]
    pub fn remote_capabilities(&self) -> &[u8] {
        self.protocol.remote_capabilities()
    }

    /// Reports whether a capability bit is present in both peers' first capability byte.
    #[must_use]
    pub fn has_negotiated_capability(&self, capability: u8) -> bool {
        self.protocol.has_negotiated_capability(capability)
    }

    /// Queues a mosh-go session-control extension after capability negotiation.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime cannot accept input or the peer did not
    /// negotiate the session-control capability.
    pub fn queue_session_control(
        &mut self,
        kind: u32,
        payload: Vec<u8>,
    ) -> Result<(), RuntimeError> {
        self.ensure_accepting_input()?;
        if !self.has_negotiated_capability(SESSION_CONTROL_CAPABILITY) {
            return Err(RuntimeError::CapabilityNotNegotiated);
        }
        let new_size = self
            .queued_input_bytes
            .checked_add(payload.len())
            .ok_or(RuntimeError::InputQueueFull)?;
        if new_size > MAXIMUM_QUEUED_INPUT_BYTES {
            return Err(RuntimeError::InputQueueFull);
        }
        self.queued_input_bytes = new_size;
        self.queued_instructions.push_back(Instruction {
            bytes: None,
            viewport: None,
            marker: None,
            session_control: Some(SessionControl { kind, payload }),
        });
        Ok(())
    }

    /// Queues a negotiated mosh-go session-list request.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime cannot accept input or session control
    /// was not negotiated.
    pub fn queue_session_list_request(&mut self) -> Result<(), RuntimeError> {
        self.queue_session_control(SESSION_LIST_REQUEST, Vec::new())
    }

    /// Queues a negotiated mosh-go session-list response with host-defined payload bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime cannot accept input or session control
    /// was not negotiated.
    pub fn queue_session_list_response(&mut self, payload: Vec<u8>) -> Result<(), RuntimeError> {
        self.queue_session_control(SESSION_LIST_RESPONSE, payload)
    }

    /// Queues a negotiated mosh-go session-switch request with host-defined payload bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime cannot accept input or session control
    /// was not negotiated.
    pub fn queue_session_switch_request(&mut self, payload: Vec<u8>) -> Result<(), RuntimeError> {
        self.queue_session_control(SESSION_SWITCH_REQUEST, payload)
    }

    /// Queues a negotiated mosh-go session-switch completion with host-defined payload bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime cannot accept input or session control
    /// was not negotiated.
    pub fn queue_session_switched(&mut self, payload: Vec<u8>) -> Result<(), RuntimeError> {
        self.queue_session_control(SESSION_SWITCHED, payload)
    }

    /// Queues a negotiated mosh-go session-create request with host-defined payload bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime cannot accept input or session control
    /// was not negotiated.
    pub fn queue_session_create_request(&mut self, payload: Vec<u8>) -> Result<(), RuntimeError> {
        self.queue_session_control(SESSION_CREATE_REQUEST, payload)
    }

    /// Queues a negotiated mosh-go session-create completion with host-defined payload bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime cannot accept input or session control
    /// was not negotiated.
    pub fn queue_session_created(&mut self, payload: Vec<u8>) -> Result<(), RuntimeError> {
        self.queue_session_control(SESSION_CREATED, payload)
    }

    /// Pauses timers and rejects new host input until `resume` is called.
    #[must_use]
    pub fn pause(&mut self) -> Vec<SessionAction> {
        if self.lifecycle != SessionLifecycle::Running {
            return Vec::new();
        }
        self.lifecycle = SessionLifecycle::Paused;
        lifecycle_actions(SessionLifecycle::Paused)
    }

    /// Cancels further protocol work and clears queued, unsent host input.
    ///
    /// The embedding host should drop the runtime after applying the returned
    /// event so its in-memory session key is released immediately.
    #[must_use]
    pub fn cancel(&mut self) -> Vec<SessionAction> {
        if self.lifecycle == SessionLifecycle::Cancelled {
            return Vec::new();
        }
        self.lifecycle = SessionLifecycle::Cancelled;
        self.queued_instructions.clear();
        self.queued_input_bytes = 0;
        lifecycle_actions(SessionLifecycle::Cancelled)
    }

    /// Starts the bounded SSP clean-shutdown handshake.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime is paused or cancelled.
    pub fn request_shutdown(
        &mut self,
        now: MonotonicTime,
    ) -> Result<Vec<SessionAction>, RuntimeError> {
        self.ensure_running()?;
        if self.shutdown == ShutdownState::Open {
            self.shutdown = ShutdownState::LocalRequested {
                started_at: now,
                transmissions: 0,
            };
            return Ok(vec![SessionAction::Diagnostic(
                DiagnosticEvent::ShutdownStarted,
            )]);
        }
        Ok(Vec::new())
    }

    /// Reports a host-managed local UDP rebind without changing the trusted peer.
    #[must_use]
    pub fn notify_udp_rebound(&mut self) -> Vec<SessionAction> {
        if self.lifecycle == SessionLifecycle::Cancelled {
            return Vec::new();
        }
        self.udp_binding_generation = self.udp_binding_generation.saturating_add(1);
        self.force_next_send();
        vec![
            SessionAction::UdpBindingChanged(self.udp_binding_generation),
            SessionAction::Diagnostic(DiagnosticEvent::UdpBindingChanged {
                generation: self.udp_binding_generation,
            }),
        ]
    }

    /// Re-arms liveness after the host reports a system resume or another
    /// interval in which network progress could not have been observed.
    pub fn resume(&mut self, now: MonotonicTime) {
        self.resume_state(now);
    }

    /// Resumes a paused session and returns the structured lifecycle event.
    #[must_use]
    pub fn resume_with_actions(&mut self, now: MonotonicTime) -> Vec<SessionAction> {
        if self.lifecycle == SessionLifecycle::Cancelled {
            return Vec::new();
        }
        let was_paused = self.lifecycle == SessionLifecycle::Paused;
        self.resume_state(now);
        if was_paused {
            lifecycle_actions(SessionLifecycle::Running)
        } else {
            Vec::new()
        }
    }

    /// Makes the next `poll` send immediately without changing SSP state.
    ///
    /// This mirrors mosh-go's `ForceNextSend` for embedding transports that
    /// have become writable again and want to re-drive the existing session.
    pub fn force_next_send(&mut self) {
        if self.lifecycle == SessionLifecycle::Running
            && !matches!(self.shutdown, ShutdownState::Complete(_))
        {
            self.last_send = None;
        }
    }

    fn resume_state(&mut self, now: MonotonicTime) {
        if self.lifecycle == SessionLifecycle::Cancelled {
            return;
        }
        self.lifecycle = SessionLifecycle::Running;
        self.last_receive = now;
        // A resumed or rebound transport should probe immediately with its existing SSP state.
        self.force_next_send();
    }

    /// Accepts an authenticated server datagram and returns terminal effects.
    ///
    /// # Errors
    ///
    /// Returns an error for authentication, replay, framing, decompression, or
    /// unsupported message failures.
    pub fn receive_datagram(
        &mut self,
        packet: &[u8],
        now: MonotonicTime,
    ) -> Result<Vec<SessionAction>, RuntimeError> {
        Ok(self.receive_datagram_inner(packet, now)?.actions)
    }

    /// Accepts a datagram and returns its raw SSP diff without terminal actions.
    ///
    /// This mirrors mosh-go's manual raw-receive mode for hosts that reconstruct
    /// terminal state themselves. It returns `None` for heartbeats, incomplete
    /// fragments, duplicates, empty diffs, and updates that do not advance
    /// remote state.
    ///
    /// # Errors
    ///
    /// Returns an error for authentication, replay, framing, decompression, or
    /// unsupported message failures.
    pub fn receive_datagram_raw(
        &mut self,
        packet: &[u8],
        now: MonotonicTime,
    ) -> Result<Option<RawRemoteUpdate>, RuntimeError> {
        Ok(self.receive_datagram_inner(packet, now)?.raw_update)
    }

    /// Processes a raw datagram with mosh-go's best-effort receive semantics.
    ///
    /// Authentication, replay, framing, decompression, and message failures
    /// are treated as a dropped datagram and return `None`.
    #[must_use]
    pub fn receive_datagram_raw_lossy(
        &mut self,
        packet: &[u8],
        now: MonotonicTime,
    ) -> Option<RawRemoteUpdate> {
        self.receive_datagram_raw(packet, now).ok().flatten()
    }

    fn receive_datagram_inner(
        &mut self,
        packet: &[u8],
        now: MonotonicTime,
    ) -> Result<ProcessedDatagram, RuntimeError> {
        self.ensure_running()?;
        let previous_capabilities = self.protocol.remote_capabilities().to_vec();
        let receipt = self
            .protocol
            .ingest_packet(packet)
            .map_err(RuntimeError::Session)?;
        self.last_receive = now;
        let mut actions = Vec::new();
        if self.protocol.remote_capabilities() != previous_capabilities {
            actions.push(SessionAction::CapabilitiesChanged(
                self.protocol.remote_capabilities().to_vec(),
            ));
        }
        if self.connection_state != ConnectionState::Connected {
            self.connection_state = ConnectionState::Connected;
            actions.push(SessionAction::ConnectionStateChanged(
                ConnectionState::Connected,
            ));
            actions.push(SessionAction::Diagnostic(
                DiagnosticEvent::ConnectionStateChanged {
                    state: ConnectionState::Connected,
                },
            ));
        }
        if is_round_trip_sample(receipt.echoed_timestamp) {
            let elapsed = timestamp(now).wrapping_sub(receipt.echoed_timestamp);
            if elapsed <= 30_000 {
                actions.extend(self.record_round_trip_sample(elapsed));
            }
        }
        let raw_update = receipt
            .state
            .map(|state| self.apply_received_state(state, &mut actions))
            .transpose()?;
        Ok(ProcessedDatagram {
            actions,
            raw_update: raw_update.flatten(),
        })
    }

    fn apply_received_state(
        &mut self,
        state: ReceivedState,
        actions: &mut Vec<SessionAction>,
    ) -> Result<Option<RawRemoteUpdate>, RuntimeError> {
        let ReceivedState {
            packet_counter,
            advances_remote_state,
            raw_delta,
            update,
            ..
        } = state;
        self.acknowledgement_due |= advances_remote_state;
        self.received_server_state = true;
        if update.target_state == u64::MAX {
            self.peer_shutdown_acknowledgement_due = true;
            if self.shutdown == ShutdownState::Open {
                self.shutdown = ShutdownState::PeerRequested;
            }
        }
        actions.push(SessionAction::Diagnostic(
            DiagnosticEvent::InboundUpdateAccepted {
                packet_counter,
                base_state: update.base_state,
                target_state: update.target_state,
                acknowledged_state: update.acknowledged_state,
                discard_before: update.discard_before,
                delta_bytes: usize_as_u64(update.delta.len()),
                advances_remote_state,
            },
        ));
        let raw_update = if advances_remote_state && !raw_delta.is_empty() {
            Some(RawRemoteUpdate {
                old_num: update.base_state,
                new_num: update.target_state,
                ack_num: update.acknowledged_state,
                throwaway_num: update.discard_before,
                diff: raw_delta,
            })
        } else {
            None
        };
        if !advances_remote_state {
            return Ok(raw_update);
        }
        actions.push(SessionAction::RemoteStateAdvanced(update.target_state));
        let instructions = update
            .decode_instructions()
            .map_err(RuntimeError::Message)?;
        actions.extend(
            instructions
                .instructions
                .into_iter()
                .flat_map(|instruction| {
                    let mut actions = Vec::with_capacity(2);
                    if let Some(bytes) = instruction.bytes {
                        actions.push(SessionAction::WriteTerminal(bytes.value));
                    }
                    if let Some(viewport) = instruction.viewport {
                        let dimensions = (
                            u16::try_from(viewport.columns),
                            u16::try_from(viewport.rows),
                        );
                        if let (Ok(columns), Ok(rows)) = dimensions {
                            actions.push(SessionAction::ResizeTerminal { columns, rows });
                        }
                    }
                    if let Some(marker) = instruction.marker {
                        actions.push(SessionAction::AcknowledgePrediction(marker.value));
                    }
                    if let Some(control) = instruction.session_control {
                        actions.push(SessionAction::RemoteSessionControl {
                            kind: control.kind,
                            payload: control.payload,
                        });
                    }
                    actions
                }),
        );
        Ok(raw_update)
    }

    /// Processes a datagram with mosh-go's best-effort receive semantics.
    ///
    /// Authentication, replay, framing, decompression, and message failures
    /// are treated as a dropped datagram and produce no host actions.
    #[must_use]
    pub fn receive_datagram_lossy(
        &mut self,
        packet: &[u8],
        now: MonotonicTime,
    ) -> Vec<SessionAction> {
        self.receive_datagram(packet, now).unwrap_or_default()
    }

    /// Advances timers and returns datagrams that should be sent now.
    ///
    /// # Errors
    ///
    /// Returns an error for protocol encoding failure.
    pub fn poll(&mut self, now: MonotonicTime) -> Result<Vec<SessionAction>, RuntimeError> {
        if self.lifecycle != SessionLifecycle::Running {
            return Ok(Vec::new());
        }
        if matches!(self.shutdown, ShutdownState::Complete(_)) {
            return Ok(Vec::new());
        }
        let mut actions = Vec::new();
        if self.association == AssociationState::Due {
            let packets = self
                .protocol
                .build_association(timestamp(now))
                .map_err(RuntimeError::Session)?;
            let diagnostic = fresh_update_diagnostic(0, &packets, 0, 0);
            self.association = AssociationState::Sent;
            self.last_send = Some(now);
            actions.extend(send_actions(packets));
            actions.push(SessionAction::Diagnostic(diagnostic));
        }
        if self.protocol.local_shutdown_acknowledged() && !self.peer_shutdown_acknowledgement_due {
            return Ok(self.complete_shutdown(ShutdownOutcome::Acknowledged));
        }
        if !self.peer_shutdown_acknowledgement_due && self.shutdown_timed_out(now) {
            return Ok(self.complete_shutdown(ShutdownOutcome::TimedOut));
        }
        actions.extend(
            self.connection_state_action(now)
                .into_iter()
                .flat_map(|state| {
                    [
                        SessionAction::ConnectionStateChanged(state),
                        SessionAction::Diagnostic(DiagnosticEvent::ConnectionStateChanged {
                            state,
                        }),
                    ]
                }),
        );
        let has_queued_state = !self.queued_instructions.is_empty();
        let local_shutdown_unsent = matches!(self.shutdown, ShutdownState::LocalRequested { .. })
            && !self.protocol.local_shutdown_sent();
        if (has_queued_state || local_shutdown_unsent) && !self.protocol.has_pending_update() {
            let (packets, diagnostic) = self.build_queued_update(now)?;
            actions.extend(send_actions(packets));
            actions.push(SessionAction::Diagnostic(diagnostic));
            self.record_shutdown_transmission();
            self.complete_peer_shutdown_after_ack(&mut actions);
            return Ok(actions);
        }

        if self.protocol.has_pending_update() {
            if !self.acknowledgement_due {
                if let Some(last_send) = self.last_send {
                    if now.elapsed_since(last_send) < self.retransmit_milliseconds {
                        return Ok(actions);
                    }
                }
            }
            let packets = self
                .protocol
                .retransmit_pending(timestamp(now))
                .map_err(RuntimeError::Session)?;
            let diagnostic = retransmission_diagnostic(
                self.protocol.latest_local_state(),
                &packets,
                self.retransmit_milliseconds,
            );
            self.last_send = Some(now);
            self.acknowledgement_due = false;
            actions.extend(send_actions(packets));
            actions.push(SessionAction::Diagnostic(diagnostic));
            self.record_shutdown_transmission();
            self.complete_peer_shutdown_after_ack(&mut actions);
            return Ok(actions);
        }

        let heartbeat_due = self
            .last_send
            .is_none_or(|last_send| now.elapsed_since(last_send) >= self.retransmit_milliseconds);
        if !self.acknowledgement_due && !heartbeat_due {
            return Ok(actions);
        }
        let packets = self
            .protocol
            .build_acknowledgement(timestamp(now))
            .map_err(RuntimeError::Session)?;
        let diagnostic =
            fresh_update_diagnostic(self.protocol.latest_local_state(), &packets, 0, 0);
        self.last_send = Some(now);
        self.acknowledgement_due = false;
        actions.extend(send_actions(packets));
        actions.push(SessionAction::Diagnostic(diagnostic));
        self.complete_peer_shutdown_after_ack(&mut actions);
        Ok(actions)
    }

    fn build_queued_update(
        &mut self,
        now: MonotonicTime,
    ) -> Result<(Vec<Vec<u8>>, DiagnosticEvent), RuntimeError> {
        let state_id = if (matches!(self.shutdown, ShutdownState::LocalRequested { .. })
            && !self.protocol.local_shutdown_sent())
            || (self.peer_shutdown_acknowledgement_due
                && self.protocol.local_shutdown_acknowledged())
        {
            u64::MAX
        } else {
            self.protocol.next_local_state()
        };
        let input_bytes = self.queued_input_bytes;
        let instructions = InstructionBatch {
            instructions: self.queued_instructions.drain(..).collect(),
        };
        let instruction_count = instructions.instructions.len();
        self.queued_input_bytes = 0;
        self.acknowledgement_due = false;
        let packets = if self.peer_shutdown_acknowledgement_due
            && self.protocol.local_shutdown_acknowledged()
        {
            self.protocol
                .build_shutdown_ack(timestamp(now))
                .map_err(RuntimeError::Session)?
        } else if matches!(self.shutdown, ShutdownState::LocalRequested { .. })
            && !self.protocol.local_shutdown_sent()
        {
            self.protocol
                .build_shutdown(timestamp(now), &instructions)
                .map_err(RuntimeError::Session)?
        } else {
            self.protocol
                .build_update(timestamp(now), &instructions)
                .map_err(RuntimeError::Session)?
        };
        self.last_send = Some(now);
        let diagnostic =
            fresh_update_diagnostic(state_id, &packets, instruction_count, input_bytes);
        Ok((packets, diagnostic))
    }

    #[must_use]
    pub fn milliseconds_until_next_poll(&self, now: MonotonicTime) -> u64 {
        if self.lifecycle != SessionLifecycle::Running
            || matches!(self.shutdown, ShutdownState::Complete(_))
        {
            return u64::MAX;
        }
        let can_send_queued = !self.protocol.has_pending_update();
        let protocol_wait = if self.association == AssociationState::Due
            || self.acknowledgement_due
            || (can_send_queued && !self.queued_instructions.is_empty())
            || (matches!(self.shutdown, ShutdownState::LocalRequested { .. })
                && !self.protocol.local_shutdown_sent()
                && can_send_queued)
        {
            0
        } else if self.protocol.has_pending_update() {
            self.last_send.map_or(0, |last_send| {
                self.retransmit_milliseconds
                    .saturating_sub(now.elapsed_since(last_send))
            })
        } else {
            self.last_send.map_or(0, |last_send| {
                self.retransmit_milliseconds
                    .saturating_sub(now.elapsed_since(last_send))
            })
        };
        match self.shutdown {
            ShutdownState::LocalRequested {
                started_at,
                transmissions,
            } => {
                let timeout_wait = if transmissions >= SHUTDOWN_RETRY_LIMIT {
                    0
                } else {
                    SHUTDOWN_TIMEOUT_MILLISECONDS.saturating_sub(now.elapsed_since(started_at))
                };
                protocol_wait.min(timeout_wait)
            }
            _ => protocol_wait,
        }
    }

    #[must_use]
    pub const fn has_received_server_state(&self) -> bool {
        self.received_server_state
    }

    /// Reports whether the server has responded recently without ending an
    /// otherwise recoverable Mosh session when connectivity is intermittent.
    #[must_use]
    pub fn is_server_responsive(&self, now: MonotonicTime) -> bool {
        now.elapsed_since(self.last_receive) < SERVER_UNRESPONSIVE_MILLISECONDS
    }

    #[must_use]
    pub const fn round_trip_milliseconds(&self) -> Option<u16> {
        self.round_trip_milliseconds
    }

    /// Returns a content-free snapshot equivalent to mosh-go's transport queries.
    #[must_use]
    pub fn transport_status(&self) -> TransportStatus {
        let state_numbers = self.protocol.transport_state_numbers();
        TransportStatus {
            acked_by_remote: state_numbers.acked_by_remote,
            sent_num: state_numbers.sent_num,
            last_recv_old_num: state_numbers.last_recv_old_num,
            last_recv_new_num: state_numbers.last_recv_new_num,
            throwaway_num: state_numbers.throwaway_num,
            retransmit_timeout_milliseconds: self.retransmit_milliseconds,
        }
    }

    /// Returns the SSP frame that will contain input queued before the next poll.
    #[must_use]
    pub fn prediction_frame_id(&self) -> u64 {
        self.protocol.next_local_state()
    }

    #[must_use]
    pub const fn connection_state(&self) -> ConnectionState {
        self.connection_state
    }

    #[must_use]
    pub const fn lifecycle(&self) -> SessionLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn shutdown_outcome(&self) -> Option<ShutdownOutcome> {
        match self.shutdown {
            ShutdownState::Complete(outcome) => Some(outcome),
            _ => None,
        }
    }

    #[must_use]
    pub const fn shutdown_in_progress(&self) -> bool {
        !matches!(
            self.shutdown,
            ShutdownState::Open | ShutdownState::Complete(_)
        )
    }

    /// Returns the time since the most recent authenticated server datagram.
    #[must_use]
    pub fn milliseconds_since_server_response(&self, now: MonotonicTime) -> u64 {
        now.elapsed_since(self.last_receive)
    }

    fn connection_state_action(&mut self, now: MonotonicTime) -> Option<ConnectionState> {
        if self.connection_state == ConnectionState::Connected && !self.is_server_responsive(now) {
            self.connection_state = ConnectionState::Interrupted;
            return Some(ConnectionState::Interrupted);
        }
        None
    }

    fn update_retransmit_timeout(&mut self, sample_milliseconds: u64) {
        if let Some(smoothed) = self.smoothed_round_trip_milliseconds {
            let difference = smoothed.abs_diff(sample_milliseconds);
            self.round_trip_variation_milliseconds = self
                .round_trip_variation_milliseconds
                .saturating_mul(3)
                .saturating_add(difference)
                / 4;
            self.smoothed_round_trip_milliseconds = Some(
                smoothed
                    .saturating_mul(7)
                    .saturating_add(sample_milliseconds)
                    / 8,
            );
        } else {
            self.smoothed_round_trip_milliseconds = Some(sample_milliseconds);
            self.round_trip_variation_milliseconds = sample_milliseconds / 2;
        }
        let smoothed = self
            .smoothed_round_trip_milliseconds
            .unwrap_or(sample_milliseconds);
        self.retransmit_milliseconds = smoothed
            .saturating_add(self.round_trip_variation_milliseconds.saturating_mul(4))
            .clamp(
                MINIMUM_RETRANSMIT_MILLISECONDS,
                MAXIMUM_RETRANSMIT_MILLISECONDS,
            );
    }

    fn record_round_trip_sample(&mut self, milliseconds: u16) -> Vec<SessionAction> {
        self.update_retransmit_timeout(u64::from(milliseconds));
        if self.round_trip_milliseconds == Some(milliseconds) {
            return Vec::new();
        }
        self.round_trip_milliseconds = Some(milliseconds);
        vec![
            SessionAction::RoundTripEstimate(milliseconds),
            SessionAction::Diagnostic(DiagnosticEvent::RoundTripUpdated { milliseconds }),
        ]
    }

    fn ensure_running(&self) -> Result<(), RuntimeError> {
        match self.lifecycle {
            SessionLifecycle::Running => Ok(()),
            SessionLifecycle::Paused => Err(RuntimeError::SessionPaused),
            SessionLifecycle::Cancelled => Err(RuntimeError::SessionCancelled),
        }
    }

    fn ensure_accepting_input(&self) -> Result<(), RuntimeError> {
        self.ensure_running()?;
        if self.shutdown != ShutdownState::Open {
            return Err(RuntimeError::ShutdownInProgress);
        }
        Ok(())
    }

    fn shutdown_timed_out(&self, now: MonotonicTime) -> bool {
        let ShutdownState::LocalRequested {
            started_at,
            transmissions,
        } = self.shutdown
        else {
            return false;
        };
        transmissions >= SHUTDOWN_RETRY_LIMIT
            || now.elapsed_since(started_at) >= SHUTDOWN_TIMEOUT_MILLISECONDS
    }

    fn record_shutdown_transmission(&mut self) {
        if let ShutdownState::LocalRequested { transmissions, .. } = &mut self.shutdown {
            *transmissions = transmissions.saturating_add(1);
        }
    }

    fn complete_peer_shutdown_after_ack(&mut self, actions: &mut Vec<SessionAction>) {
        if self.peer_shutdown_acknowledgement_due {
            self.peer_shutdown_acknowledgement_due = false;
            actions.extend(self.complete_shutdown(ShutdownOutcome::PeerRequested));
        }
    }

    fn complete_shutdown(&mut self, outcome: ShutdownOutcome) -> Vec<SessionAction> {
        self.shutdown = ShutdownState::Complete(outcome);
        vec![
            SessionAction::ShutdownComplete(outcome),
            SessionAction::Diagnostic(DiagnosticEvent::ShutdownComplete { outcome }),
        ]
    }
}

fn lifecycle_actions(state: SessionLifecycle) -> Vec<SessionAction> {
    vec![
        SessionAction::SessionLifecycleChanged(state),
        SessionAction::Diagnostic(DiagnosticEvent::SessionLifecycleChanged { state }),
    ]
}

fn fresh_update_diagnostic(
    state_id: u64,
    packets: &[Vec<u8>],
    instruction_count: usize,
    input_bytes: usize,
) -> DiagnosticEvent {
    DiagnosticEvent::FreshUpdatePrepared {
        state_id,
        datagram_count: usize_as_u64(packets.len()),
        datagram_bytes: total_packet_bytes(packets),
        instruction_count: usize_as_u64(instruction_count),
        input_bytes: usize_as_u64(input_bytes),
    }
}

fn retransmission_diagnostic(
    state_id: u64,
    packets: &[Vec<u8>],
    retransmit_delay_milliseconds: u64,
) -> DiagnosticEvent {
    DiagnosticEvent::RetransmissionPrepared {
        state_id,
        datagram_count: usize_as_u64(packets.len()),
        datagram_bytes: total_packet_bytes(packets),
        retransmit_delay_milliseconds,
    }
}

fn total_packet_bytes(packets: &[Vec<u8>]) -> u64 {
    packets.iter().fold(0_u64, |total, packet| {
        total.saturating_add(usize_as_u64(packet.len()))
    })
}

fn usize_as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn timestamp(now: MonotonicTime) -> u16 {
    let low_bits = now.milliseconds() & u64::from(u16::MAX);
    u16::try_from(low_bits).expect("timestamp is masked to 16 bits")
}

fn is_round_trip_sample(echoed_timestamp: u16) -> bool {
    // mosh-go uses zero before it has a peer timestamp, while standard Mosh
    // commonly uses the all-ones sentinel. Neither value is an RTT sample.
    !matches!(
        echoed_timestamp,
        MOSH_GO_NO_ECHO_TIMESTAMP | STANDARD_MOSH_NO_ECHO_TIMESTAMP
    )
}

fn send_actions(packets: Vec<Vec<u8>>) -> Vec<SessionAction> {
    packets
        .into_iter()
        .map(SessionAction::SendDatagram)
        .collect()
}

#[derive(Debug)]
pub enum RuntimeError {
    InputQueueFull,
    CapabilityNotNegotiated,
    SessionPaused,
    SessionCancelled,
    ShutdownInProgress,
    Session(SessionError),
    Message(MessageError),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputQueueFull => formatter.write_str("terminal input queue is full"),
            Self::CapabilityNotNegotiated => {
                formatter.write_str("session-control capability was not negotiated")
            }
            Self::SessionPaused => formatter.write_str("session is paused"),
            Self::SessionCancelled => formatter.write_str("session is cancelled"),
            Self::ShutdownInProgress => formatter.write_str("session shutdown is in progress"),
            Self::Session(error) => write!(formatter, "session protocol failed: {error:?}"),
            Self::Message(error) => write!(formatter, "session message failed: {error:?}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

#[cfg(test)]
mod tests {
    use super::{
        ConnectionState, DiagnosticEvent, MonotonicTime, RawRemoteUpdate, RuntimeError,
        SessionAction, SessionLifecycle, SessionRuntime, ShutdownOutcome, TerminalInputEvent,
        TransportStatus,
    };
    use fernomade_crypto::{PeerRole, SecureChannel, SessionKey};
    use fernomade_wire::{
        ByteRun, Fragment, Instruction, InstructionBatch, SESSION_CONTROL_CAPABILITY,
        SessionControl, StateUpdate, ViewportSize, decode_compressed_update,
        encode_compressed_update,
    };

    const SYNTHETIC_KEY: &str = "AAECAwQFBgcICQoLDA0ODw";

    #[test]
    fn immediate_acknowledgement_and_retransmission_are_deterministic() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        assert!(!runtime.has_received_server_state());
        runtime.queue_resize(80, 24);
        let first = runtime.poll(time(0)).expect("initial poll must send");
        assert_eq!(non_diagnostic_actions(first).len(), 2);
        assert!(
            runtime
                .poll(time(999))
                .expect("early poll must work")
                .is_empty()
        );
        assert_eq!(
            non_diagnostic_actions(runtime.poll(time(1_000)).expect("retry must send")).len(),
            1
        );
        assert_eq!(runtime.milliseconds_until_next_poll(time(1_000)), 1_000);

        let server_packet = server_packet_with_echoed_timestamp(1, b"ready", 1);
        assert_eq!(
            non_diagnostic_actions(
                runtime
                    .receive_datagram(&server_packet, time(300))
                    .expect("server state must open")
            ),
            vec![
                SessionAction::ConnectionStateChanged(ConnectionState::Connected),
                SessionAction::RoundTripEstimate(299),
                SessionAction::RemoteStateAdvanced(1),
                SessionAction::WriteTerminal(b"ready".to_vec()),
            ]
        );
        assert!(runtime.has_received_server_state());
        assert_eq!(runtime.milliseconds_until_next_poll(time(300)), 0);
        assert_eq!(
            non_diagnostic_actions(runtime.poll(time(300)).expect("ack must send")).len(),
            1
        );
    }

    #[test]
    fn input_is_bounded_and_network_silence_is_recoverable() {
        let mut runtime = SessionRuntime::new(key(), time(10));
        assert!(matches!(
            runtime.queue_input(vec![0; 1024 * 1024 + 1]),
            Err(RuntimeError::InputQueueFull)
        ));
        assert!(!runtime.is_server_responsive(time(30_010)));
        assert!(runtime.poll(time(30_010)).is_ok());
    }

    #[test]
    fn rapid_resize_preserves_each_mosh_go_dimension_change() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        runtime.queue_resize(80, 24);
        runtime.queue_resize(100, 30);
        runtime.queue_resize(132, 43);
        let actions = runtime.poll(time(0)).expect("resize must send");
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());
        let instructions = decode_sent_update(&actions, &mut server_channel)
            .decode_instructions()
            .expect("resize instructions must decode")
            .instructions;

        assert_eq!(instructions.len(), 3);
        assert_eq!(
            instructions[0].viewport,
            Some(ViewportSize {
                columns: 80,
                rows: 24,
            })
        );
        assert_eq!(
            instructions[1].viewport,
            Some(ViewportSize {
                columns: 100,
                rows: 30,
            })
        );
        assert_eq!(
            instructions[2].viewport,
            Some(ViewportSize {
                columns: 132,
                rows: 43,
            })
        );
    }

    #[test]
    fn resize_events_preserve_mosh_go_input_order() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        runtime
            .queue_input(b"a".to_vec())
            .expect("input must queue");
        runtime.queue_resize(80, 24);
        runtime.queue_resize(100, 30);
        runtime
            .queue_input(b"b".to_vec())
            .expect("input must queue");
        runtime.queue_resize(132, 43);

        let actions = runtime.poll(time(0)).expect("queued state must send");
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());
        let instructions = decode_sent_update(&actions, &mut server_channel)
            .decode_instructions()
            .expect("instructions must decode")
            .instructions;

        assert_eq!(instructions.len(), 5);
        assert_eq!(
            instructions[0]
                .bytes
                .as_ref()
                .map(|bytes| bytes.value.as_slice()),
            Some(b"a".as_slice())
        );
        assert_eq!(
            instructions[1].viewport,
            Some(ViewportSize {
                columns: 80,
                rows: 24
            })
        );
        assert_eq!(
            instructions[2].viewport,
            Some(ViewportSize {
                columns: 100,
                rows: 30
            })
        );
        assert_eq!(
            instructions[3]
                .bytes
                .as_ref()
                .map(|bytes| bytes.value.as_slice()),
            Some(b"b".as_slice())
        );
        assert_eq!(
            instructions[4].viewport,
            Some(ViewportSize {
                columns: 132,
                rows: 43
            })
        );
    }

    #[test]
    fn round_trip_samples_drive_mosh_go_retransmit_bounds() {
        let mut fast = SessionRuntime::new(key(), time(0));
        fast.update_retransmit_timeout(0);
        assert_eq!(fast.retransmit_milliseconds, 250);

        let mut typical = SessionRuntime::new(key(), time(0));
        typical.update_retransmit_timeout(100);
        assert_eq!(typical.retransmit_milliseconds, 300);
        typical.update_retransmit_timeout(100);
        assert_eq!(typical.retransmit_milliseconds, 250);

        let mut slow = SessionRuntime::new(key(), time(0));
        slow.update_retransmit_timeout(10_000);
        assert_eq!(slow.retransmit_milliseconds, 10_000);
    }

    #[test]
    fn timestamp_only_heartbeat_updates_liveness_and_round_trip_time() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let heartbeat = SecureChannel::new(PeerRole::Server, key())
            .seal_next(&[0, 77, 0, 10])
            .expect("heartbeat must seal");

        assert_eq!(
            non_diagnostic_actions(
                runtime
                    .receive_datagram(&heartbeat, time(110))
                    .expect("heartbeat must authenticate")
            ),
            vec![
                SessionAction::ConnectionStateChanged(ConnectionState::Connected),
                SessionAction::RoundTripEstimate(100),
            ]
        );
        assert!(!runtime.has_received_server_state());
        assert!(runtime.is_server_responsive(time(110)));

        let actions = runtime.poll(time(110)).expect("association must send");
        let packet = actions
            .iter()
            .find_map(|action| match action {
                SessionAction::SendDatagram(packet) => Some(packet),
                _ => None,
            })
            .expect("poll must produce an association datagram");
        let plaintext = SecureChannel::new(PeerRole::Server, key())
            .open(packet)
            .expect("association must authenticate")
            .plaintext;
        let fragment = Fragment::parse(&plaintext).expect("association must contain a state");
        assert_eq!(fragment.header.echoed_timestamp, 77);
    }

    #[test]
    fn no_echo_sentinels_do_not_produce_round_trip_samples() {
        for echoed_timestamp in [
            super::MOSH_GO_NO_ECHO_TIMESTAMP,
            super::STANDARD_MOSH_NO_ECHO_TIMESTAMP,
        ] {
            let mut runtime = SessionRuntime::new(key(), time(0));
            let packet = server_packet_with_echoed_timestamp(0, b"ready", echoed_timestamp);

            let actions = runtime
                .receive_datagram(&packet, time(250))
                .expect("authenticated server state must open");
            assert!(
                actions
                    .iter()
                    .all(|action| !matches!(action, SessionAction::RoundTripEstimate(_)))
            );
            assert_eq!(runtime.round_trip_milliseconds(), None);
        }
    }

    #[test]
    fn negotiated_session_control_is_exposed_and_can_be_queued() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        runtime.set_local_capabilities(vec![SESSION_CONTROL_CAPABILITY]);

        let mut update = StateUpdate::new(0, 1, 0);
        update.capabilities = vec![SESSION_CONTROL_CAPABILITY];
        update.delta = InstructionBatch {
            instructions: vec![Instruction {
                bytes: None,
                viewport: None,
                marker: None,
                session_control: Some(SessionControl {
                    kind: 2,
                    payload: b"remote".to_vec(),
                }),
            }],
        }
        .encode_bytes();
        let packet = encoded_server_update(&update, 1);
        let inbound = runtime
            .receive_datagram(&packet, time(1))
            .expect("extension update must open");

        assert!(runtime.has_negotiated_capability(SESSION_CONTROL_CAPABILITY));
        assert!(inbound.contains(&SessionAction::CapabilitiesChanged(vec![
            SESSION_CONTROL_CAPABILITY
        ])));
        assert!(inbound.contains(&SessionAction::RemoteSessionControl {
            kind: 2,
            payload: b"remote".to_vec(),
        }));

        runtime
            .queue_session_control(3, b"local".to_vec())
            .expect("negotiated control must queue");
        let outbound = runtime.poll(time(1)).expect("control must send");
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());
        let instructions = decode_sent_update(&outbound, &mut server_channel)
            .decode_instructions()
            .expect("control must decode")
            .instructions;
        assert_eq!(
            instructions
                .last()
                .and_then(|instruction| instruction.session_control.as_ref()),
            Some(&SessionControl {
                kind: 3,
                payload: b"local".to_vec()
            })
        );
    }

    #[test]
    fn typed_session_control_helpers_preserve_mosh_go_control_kinds() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        runtime.set_local_capabilities(vec![SESSION_CONTROL_CAPABILITY]);
        let mut update = StateUpdate::new(0, 1, 0);
        update.capabilities = vec![SESSION_CONTROL_CAPABILITY];
        let packet = encoded_server_update(&update, 1);
        runtime
            .receive_datagram(&packet, time(1))
            .expect("capability state must open");

        runtime
            .queue_session_list_request()
            .expect("list request must queue");
        runtime
            .queue_session_list_response(b"list".to_vec())
            .expect("list response must queue");
        runtime
            .queue_session_switch_request(b"switch".to_vec())
            .expect("switch request must queue");
        runtime
            .queue_session_switched(b"switched".to_vec())
            .expect("switch completion must queue");
        runtime
            .queue_session_create_request(b"create".to_vec())
            .expect("create request must queue");
        runtime
            .queue_session_created(b"created".to_vec())
            .expect("create completion must queue");

        let actions = runtime.poll(time(1)).expect("controls must send");
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());
        let instructions = decode_sent_update(&actions, &mut server_channel)
            .decode_instructions()
            .expect("controls must decode")
            .instructions;
        let controls = instructions
            .iter()
            .map(|instruction| {
                let control = instruction
                    .session_control
                    .as_ref()
                    .expect("control instruction must exist");
                (control.kind, control.payload.as_slice())
            })
            .collect::<Vec<_>>();

        assert_eq!(
            controls,
            vec![
                (1, b"".as_slice()),
                (2, b"list".as_slice()),
                (3, b"switch".as_slice()),
                (4, b"switched".as_slice()),
                (5, b"create".as_slice()),
                (6, b"created".as_slice()),
            ]
        );
    }

    #[test]
    fn classic_mosh_peer_leaves_session_control_unnegotiated() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        runtime.set_local_capabilities(vec![SESSION_CONTROL_CAPABILITY]);

        let mut update = StateUpdate::new(0, 1, 0);
        update.delta = InstructionBatch {
            instructions: vec![Instruction {
                bytes: Some(ByteRun {
                    value: b"classic output".to_vec(),
                }),
                viewport: None,
                marker: None,
                session_control: None,
            }],
        }
        .encode_bytes();
        let packet = encoded_server_update(&update, 1);

        let actions = runtime
            .receive_datagram(&packet, time(1))
            .expect("classic state must open");

        assert_eq!(runtime.remote_capabilities(), &[]);
        assert!(!runtime.has_negotiated_capability(SESSION_CONTROL_CAPABILITY));
        assert!(actions.contains(&SessionAction::WriteTerminal(b"classic output".to_vec())));
        assert!(matches!(
            runtime.queue_session_control(1, Vec::new()),
            Err(RuntimeError::CapabilityNotNegotiated)
        ));
    }

    #[test]
    fn transport_status_matches_accepted_ssp_state_boundaries() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());

        runtime.poll(time(0)).expect("association must send");
        runtime
            .queue_input(b"local input".to_vec())
            .expect("input must queue");
        runtime.poll(time(1)).expect("local update must send");

        let first_update = StateUpdate::new(0, 1, 1);
        let first_packet =
            encoded_server_update_with_channel(&mut server_channel, &first_update, 1);
        runtime
            .receive_datagram(&first_packet, time(1))
            .expect("first remote state must open");

        let mut second_update = StateUpdate::new(1, 2, 1);
        second_update.discard_before = 1;
        let second_packet =
            encoded_server_update_with_channel(&mut server_channel, &second_update, 2);
        runtime
            .receive_datagram(&second_packet, time(2))
            .expect("second remote state must open");

        assert_eq!(
            runtime.transport_status(),
            TransportStatus {
                acked_by_remote: 1,
                sent_num: 1,
                last_recv_old_num: 1,
                last_recv_new_num: 2,
                throwaway_num: 1,
                retransmit_timeout_milliseconds: 1_000,
            }
        );
        assert_eq!(runtime.milliseconds_since_server_response(time(2)), 0);
    }

    #[test]
    fn raw_receive_returns_the_original_accepted_ssp_diff() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());
        let mut update = StateUpdate::new(0, 1, 0);
        update.delta = InstructionBatch {
            instructions: vec![Instruction {
                bytes: Some(ByteRun {
                    value: b"raw terminal output".to_vec(),
                }),
                viewport: None,
                marker: None,
                session_control: None,
            }],
        }
        .encode_bytes();
        let expected_diff = update.delta.clone();
        let packet = encoded_server_update_with_channel(&mut server_channel, &update, 1);

        let raw_update = runtime
            .receive_datagram_raw(&packet, time(1))
            .expect("raw state must open")
            .expect("advancing state must return a raw diff");

        assert_eq!(
            raw_update,
            RawRemoteUpdate {
                old_num: 0,
                new_num: 1,
                ack_num: 0,
                throwaway_num: 0,
                diff: expected_diff,
            }
        );

        let duplicate = encoded_server_update_with_channel(&mut server_channel, &update, 2);
        assert_eq!(
            runtime
                .receive_datagram_raw(&duplicate, time(2))
                .expect("duplicate state must authenticate"),
            None
        );
    }

    #[test]
    fn raw_receive_suppresses_an_empty_mosh_go_diff() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let update = StateUpdate::new(0, 1, 0);
        let packet = encoded_server_update(&update, 1);

        assert_eq!(
            runtime
                .receive_datagram_raw(&packet, time(1))
                .expect("empty state must open"),
            None
        );
        assert_eq!(runtime.transport_status().last_recv_new_num, 1);
    }

    #[test]
    fn raw_lossy_receive_returns_an_accepted_nonempty_diff() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let mut update = StateUpdate::new(0, 1, 0);
        update.delta = InstructionBatch {
            instructions: vec![Instruction {
                bytes: Some(ByteRun {
                    value: b"raw lossy output".to_vec(),
                }),
                viewport: None,
                marker: None,
                session_control: None,
            }],
        }
        .encode_bytes();
        let packet = encoded_server_update(&update, 1);

        assert_eq!(
            runtime
                .receive_datagram_raw_lossy(&packet, time(1))
                .map(|raw| raw.diff),
            Some(update.delta)
        );
    }

    #[test]
    fn duplicate_remote_state_can_refresh_mosh_go_capabilities() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        runtime.set_local_capabilities(vec![0b11]);
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());

        let mut initial_update = StateUpdate::new(0, 1, 0);
        initial_update.capabilities = vec![0b01];
        let initial_packet =
            encoded_server_update_with_channel(&mut server_channel, &initial_update, 1);
        runtime
            .receive_datagram(&initial_packet, time(1))
            .expect("initial extension state must open");
        assert!(runtime.has_negotiated_capability(0b01));
        assert!(!runtime.has_negotiated_capability(0b10));

        let mut duplicate_update = StateUpdate::new(0, 1, 0);
        duplicate_update.capabilities = vec![0b11];
        let duplicate_packet =
            encoded_server_update_with_channel(&mut server_channel, &duplicate_update, 2);
        let actions = runtime
            .receive_datagram(&duplicate_packet, time(2))
            .expect("duplicate extension state must open");

        // mosh-go applies a non-empty capability advertisement before SSP deduplication.
        assert!(actions.contains(&SessionAction::CapabilitiesChanged(vec![0b11])));
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, SessionAction::RemoteStateAdvanced(1)))
        );
        assert!(runtime.has_negotiated_capability(0b10));
    }

    #[test]
    fn remote_session_control_preserves_instruction_order() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let mut update = StateUpdate::new(0, 1, 0);
        update.delta = InstructionBatch {
            instructions: vec![
                Instruction {
                    bytes: None,
                    viewport: None,
                    marker: None,
                    session_control: Some(SessionControl {
                        kind: 2,
                        payload: b"switch".to_vec(),
                    }),
                },
                Instruction {
                    bytes: Some(ByteRun {
                        value: b"after".to_vec(),
                    }),
                    viewport: None,
                    marker: None,
                    session_control: None,
                },
                Instruction {
                    bytes: None,
                    viewport: Some(ViewportSize {
                        columns: 100,
                        rows: 30,
                    }),
                    marker: None,
                    session_control: None,
                },
            ],
        }
        .encode_bytes();
        let packet = encoded_server_update(&update, 1);

        let actions = non_diagnostic_actions(
            runtime
                .receive_datagram(&packet, time(1))
                .expect("ordered update must open"),
        );
        let control_index = actions
            .iter()
            .position(|action| matches!(action, SessionAction::RemoteSessionControl { .. }))
            .expect("control action must exist");
        let output_index = actions
            .iter()
            .position(|action| matches!(action, SessionAction::WriteTerminal(_)))
            .expect("terminal action must exist");
        let resize_index = actions
            .iter()
            .position(|action| matches!(action, SessionAction::ResizeTerminal { .. }))
            .expect("resize action must exist");
        assert!(control_index < output_index && output_index < resize_index);
    }

    #[test]
    fn duplicate_remote_state_does_not_start_an_acknowledgement_loop() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        runtime.poll(time(0)).expect("association must send");
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());
        let update = StateUpdate::new(0, 1, 0);

        let first = encoded_server_update_with_channel(&mut server_channel, &update, 1);
        runtime
            .receive_datagram(&first, time(10))
            .expect("first state must open");
        runtime
            .poll(time(10))
            .expect("first state must be acknowledged");

        let duplicate = encoded_server_update_with_channel(&mut server_channel, &update, 2);
        runtime
            .receive_datagram(&duplicate, time(11))
            .expect("duplicate state must open");
        assert!(runtime.poll(time(11)).expect("poll must work").is_empty());
    }

    #[test]
    fn new_input_waits_for_the_single_in_flight_state() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());

        runtime
            .queue_input(b"a".to_vec())
            .expect("input must queue");
        let first = runtime.poll(time(0)).expect("first input must send");
        let first_update = decode_sent_update(&first, &mut server_channel);
        assert_eq!((first_update.base_state, first_update.target_state), (0, 1));

        runtime
            .queue_input(b"b".to_vec())
            .expect("input must queue");
        let second = runtime.poll(time(1)).expect("second input must send");
        assert!(second.is_empty());

        let retransmitted = runtime
            .poll(time(1_000))
            .expect("in-flight state must retry");
        let retransmitted_update = decode_sent_update(&retransmitted, &mut server_channel);
        assert_eq!(
            (
                retransmitted_update.base_state,
                retransmitted_update.target_state
            ),
            (0, 1)
        );
        let instructions = retransmitted_update
            .decode_instructions()
            .expect("retransmitted input must decode");
        assert_eq!(instructions.instructions.len(), 1);

        let acknowledgement = server_state_packet(0, 1, 1);
        runtime
            .receive_datagram(&acknowledgement, time(1_001))
            .expect("acknowledgement must open");
        let next = runtime
            .poll(time(1_001))
            .expect("queued input must send after acknowledgement");
        let next_update = decode_sent_update(&next, &mut server_channel);
        assert_eq!((next_update.base_state, next_update.target_state), (1, 2));
        assert_eq!(
            next_update
                .decode_instructions()
                .expect("next input must decode")
                .instructions
                .len(),
            1
        );
    }

    #[test]
    fn empty_input_creates_a_mosh_go_empty_instruction_state() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());

        runtime
            .queue_input(Vec::new())
            .expect("empty input must be accepted");
        let actions = runtime.poll(time(0)).expect("empty input must send");
        let update = decode_sent_update(&actions, &mut server_channel);

        assert_eq!((update.base_state, update.target_state), (0, 1));
        assert_eq!(
            update
                .decode_instructions()
                .expect("empty instruction must decode")
                .instructions,
            vec![Instruction {
                bytes: Some(ByteRun { value: Vec::new() }),
                viewport: None,
                marker: None,
                session_control: None,
            }]
        );
        assert_eq!(runtime.transport_status().sent_num, 1);
        assert_eq!(runtime.prediction_frame_id(), 2);
    }

    #[test]
    fn resume_prevents_sleep_interval_from_becoming_server_timeout() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        runtime.queue_resize(80, 24);
        assert_eq!(
            non_diagnostic_actions(runtime.poll(time(0)).expect("initial state must send")).len(),
            2
        );

        runtime.resume(time(120_000));
        assert_eq!(
            non_diagnostic_actions(runtime.poll(time(120_000)).expect("resume poll must work"))
                .len(),
            1
        );
        assert_eq!(runtime.milliseconds_until_next_poll(time(120_000)), 1_000);
    }

    #[test]
    fn force_next_send_rearms_an_idle_heartbeat() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());
        let association = runtime.poll(time(0)).expect("association must send");
        assert_eq!(
            decode_sent_update(&association, &mut server_channel).target_state,
            0
        );
        assert_eq!(runtime.milliseconds_until_next_poll(time(1)), 999);

        runtime.force_next_send();
        assert_eq!(runtime.milliseconds_until_next_poll(time(1)), 0);
        let heartbeat = runtime.poll(time(1)).expect("heartbeat must send");
        assert_eq!(
            heartbeat
                .iter()
                .filter(|action| !matches!(action, SessionAction::Diagnostic(_)))
                .count(),
            1
        );
        let update = decode_sent_update(&heartbeat, &mut server_channel);
        assert_eq!((update.base_state, update.target_state), (0, 0));
        assert_eq!(runtime.prediction_frame_id(), 1);
    }

    #[test]
    fn force_next_send_retransmits_pending_input_without_advancing_state() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());
        runtime
            .queue_input(b"pending".to_vec())
            .expect("input must queue");

        let first = runtime.poll(time(0)).expect("input must send");
        let first_update = decode_sent_update(&first, &mut server_channel);
        assert_eq!((first_update.base_state, first_update.target_state), (0, 1));

        runtime.force_next_send();
        let retransmission = runtime.poll(time(1)).expect("input must retransmit");
        let retransmitted_update = decode_sent_update(&retransmission, &mut server_channel);
        assert_eq!(
            (
                retransmitted_update.base_state,
                retransmitted_update.target_state
            ),
            (0, 1)
        );
        // A forced send reuses the pending SSP state instead of duplicating input.
        assert_eq!(
            retransmitted_update
                .decode_instructions()
                .expect("retransmitted input must decode")
                .instructions,
            first_update
                .decode_instructions()
                .expect("initial input must decode")
                .instructions
        );
        assert_eq!(runtime.prediction_frame_id(), 2);
    }

    #[test]
    fn server_echo_ack_is_exposed_to_the_prediction_host() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let packet = server_instruction_packet(
            0,
            Instruction {
                bytes: None,
                viewport: None,
                marker: Some(fernomade_wire::Marker { value: 17 }),
                session_control: None,
            },
        );
        assert_eq!(
            non_diagnostic_actions(
                runtime
                    .receive_datagram(&packet, time(1))
                    .expect("echo acknowledgement must decode")
            ),
            vec![
                SessionAction::ConnectionStateChanged(ConnectionState::Connected),
                SessionAction::RemoteStateAdvanced(1),
                SessionAction::AcknowledgePrediction(17),
            ]
        );
    }

    #[test]
    fn authoritative_server_resize_is_exposed_to_the_host() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let packet = server_instruction_packet(
            0,
            Instruction {
                bytes: None,
                viewport: Some(ViewportSize {
                    columns: 132,
                    rows: 43,
                }),
                marker: None,
                session_control: None,
            },
        );

        let actions = runtime
            .receive_datagram(&packet, time(1))
            .expect("server resize must decode");
        assert!(actions.contains(&SessionAction::ResizeTerminal {
            columns: 132,
            rows: 43,
        }));
    }

    #[test]
    fn connection_events_distinguish_initial_and_interrupted_sessions() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        assert_eq!(runtime.connection_state(), ConnectionState::Connecting);
        assert_eq!(runtime.milliseconds_since_server_response(time(12)), 12);
        assert!(
            runtime
                .poll(time(30_000))
                .expect("poll must work")
                .iter()
                .all(|action| !matches!(action, SessionAction::ConnectionStateChanged(_)))
        );

        let packet = server_packet(0, b"connected");
        let connected = runtime
            .receive_datagram(&packet, time(30_001))
            .expect("server state must open");
        assert!(connected.contains(&SessionAction::ConnectionStateChanged(
            ConnectionState::Connected
        )));

        let interrupted = runtime.poll(time(60_001)).expect("poll must work");
        assert!(interrupted.contains(&SessionAction::ConnectionStateChanged(
            ConnectionState::Interrupted
        )));
        assert_eq!(runtime.connection_state(), ConnectionState::Interrupted);
    }

    #[test]
    fn diagnostics_report_only_content_free_protocol_metadata() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        runtime
            .queue_input(b"CLIENT_INPUT_SENTINEL".to_vec())
            .expect("sentinel input must queue");
        let outbound = runtime.poll(time(0)).expect("input must produce an update");
        assert!(outbound.iter().any(|action| matches!(
            action,
            SessionAction::Diagnostic(DiagnosticEvent::FreshUpdatePrepared {
                state_id: 1,
                datagram_count: 1,
                instruction_count: 1,
                input_bytes: 21,
                ..
            })
        )));
        let retransmission = runtime.poll(time(1_000)).expect("update must retransmit");
        assert!(retransmission.iter().any(|action| matches!(
            action,
            SessionAction::Diagnostic(DiagnosticEvent::RetransmissionPrepared {
                state_id: 1,
                datagram_count: 1,
                retransmit_delay_milliseconds: 1_000,
                ..
            })
        )));

        let inbound_packet = server_packet(1, b"SERVER_OUTPUT_SENTINEL");
        let inbound = runtime
            .receive_datagram(&inbound_packet, time(300))
            .expect("server update must open");
        assert!(inbound.iter().any(|action| matches!(
            action,
            SessionAction::Diagnostic(DiagnosticEvent::InboundUpdateAccepted {
                packet_counter: 0,
                base_state: 0,
                target_state: 1,
                acknowledged_state: 1,
                discard_before: 0,
                advances_remote_state: true,
                ..
            })
        )));
        let rendered = format!("{outbound:?}{retransmission:?}{inbound:?}");
        assert!(!rendered.contains("CLIENT_INPUT_SENTINEL"));
        assert!(!rendered.contains("SERVER_OUTPUT_SENTINEL"));
        assert!(!rendered.contains(SYNTHETIC_KEY));
    }

    #[test]
    fn action_debug_redacts_datagram_and_terminal_bytes() {
        let datagram = SessionAction::SendDatagram(b"DATAGRAM_SENTINEL".to_vec());
        let terminal = SessionAction::WriteTerminal(b"TERMINAL_SENTINEL".to_vec());
        let rendered = format!("{datagram:?} {terminal:?}");

        assert_eq!(
            rendered,
            "SendDatagram { bytes: 17 } WriteTerminal { bytes: 17 }"
        );
        assert!(!rendered.contains("SENTINEL"));
    }

    #[test]
    fn embedding_lifecycle_and_rebind_events_are_structured() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        assert_eq!(runtime.lifecycle(), SessionLifecycle::Running);
        assert_eq!(
            runtime.pause(),
            vec![
                SessionAction::SessionLifecycleChanged(SessionLifecycle::Paused),
                SessionAction::Diagnostic(DiagnosticEvent::SessionLifecycleChanged {
                    state: SessionLifecycle::Paused,
                }),
            ]
        );
        assert!(matches!(
            runtime.queue_terminal_event(TerminalInputEvent::Bytes(b"secret input".to_vec())),
            Err(RuntimeError::SessionPaused)
        ));
        assert_eq!(runtime.milliseconds_until_next_poll(time(1)), u64::MAX);
        assert_eq!(
            runtime.resume_with_actions(time(2)),
            vec![
                SessionAction::SessionLifecycleChanged(SessionLifecycle::Running),
                SessionAction::Diagnostic(DiagnosticEvent::SessionLifecycleChanged {
                    state: SessionLifecycle::Running,
                }),
            ]
        );
        assert_eq!(
            runtime.notify_udp_rebound(),
            vec![
                SessionAction::UdpBindingChanged(1),
                SessionAction::Diagnostic(DiagnosticEvent::UdpBindingChanged { generation: 1 }),
            ]
        );
        assert_eq!(
            runtime.notify_udp_rebound()[0],
            SessionAction::UdpBindingChanged(2)
        );
    }

    #[test]
    fn cancellation_rejects_work_and_terminal_event_debug_redacts_bytes() {
        let event = TerminalInputEvent::Bytes(b"INPUT_EVENT_SENTINEL".to_vec());
        assert!(!format!("{event:?}").contains("INPUT_EVENT_SENTINEL"));

        let mut runtime = SessionRuntime::new(key(), time(0));
        runtime
            .queue_terminal_event(event)
            .expect("running session must accept host input");
        assert!(
            runtime
                .cancel()
                .contains(&SessionAction::SessionLifecycleChanged(
                    SessionLifecycle::Cancelled
                ))
        );
        assert!(matches!(
            runtime.poll(time(1)),
            Ok(actions) if actions.is_empty()
        ));
        assert!(matches!(
            runtime.queue_input(Vec::new()),
            Err(RuntimeError::SessionCancelled)
        ));
        assert!(matches!(
            runtime.receive_datagram(&[], time(1)),
            Err(RuntimeError::SessionCancelled)
        ));
        runtime.queue_resize(132, 43);
        assert!(runtime.queued_instructions.is_empty());
        assert!(runtime.notify_udp_rebound().is_empty());
    }

    #[test]
    fn local_shutdown_uses_reserved_state_and_completes_on_acknowledgement() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());
        runtime
            .queue_input(b"final".to_vec())
            .expect("final input must queue");
        assert!(
            runtime
                .request_shutdown(time(0))
                .expect("shutdown must start")
                .contains(&SessionAction::Diagnostic(DiagnosticEvent::ShutdownStarted))
        );
        assert!(matches!(
            runtime.queue_input(b"late".to_vec()),
            Err(RuntimeError::ShutdownInProgress)
        ));

        let outbound = runtime.poll(time(0)).expect("shutdown state must send");
        let update = decode_sent_update(&outbound, &mut server_channel);
        assert_eq!(update.target_state, u64::MAX);
        assert!(outbound.iter().any(|action| matches!(
            action,
            SessionAction::Diagnostic(DiagnosticEvent::FreshUpdatePrepared {
                state_id: u64::MAX,
                ..
            })
        )));

        let acknowledgement = server_state_packet(0, 1, u64::MAX);
        runtime
            .receive_datagram(&acknowledgement, time(1))
            .expect("shutdown acknowledgement must open");
        let completion = runtime.poll(time(1)).expect("shutdown must complete");
        assert!(completion.contains(&SessionAction::ShutdownComplete(
            ShutdownOutcome::Acknowledged
        )));
        assert_eq!(
            runtime.shutdown_outcome(),
            Some(ShutdownOutcome::Acknowledged)
        );
    }

    #[test]
    fn peer_shutdown_is_acknowledged_before_completion() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());
        let request = server_state_packet(0, u64::MAX, 0);
        runtime
            .receive_datagram(&request, time(1))
            .expect("peer shutdown request must open");

        let actions = runtime.poll(time(1)).expect("peer shutdown ack must send");
        let acknowledgement = decode_sent_update(&actions, &mut server_channel);
        assert_eq!(acknowledgement.acknowledged_state, u64::MAX);
        assert!(actions.contains(&SessionAction::ShutdownComplete(
            ShutdownOutcome::PeerRequested
        )));
        assert_eq!(
            runtime.shutdown_outcome(),
            Some(ShutdownOutcome::PeerRequested)
        );
    }

    #[test]
    fn simultaneous_shutdown_still_sends_the_peer_acknowledgement() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        let mut server_channel = SecureChannel::new(PeerRole::Server, key());
        runtime
            .request_shutdown(time(0))
            .expect("local shutdown must start");
        let initial = runtime.poll(time(0)).expect("local shutdown must send");
        assert_eq!(
            decode_sent_update(&initial, &mut server_channel).target_state,
            u64::MAX
        );

        let peer_request = server_state_packet(0, u64::MAX, u64::MAX);
        runtime
            .receive_datagram(&peer_request, time(1))
            .expect("simultaneous peer shutdown must open");
        let completion = runtime.poll(time(1)).expect("peer ack must send");
        let acknowledgement = decode_sent_update(&completion, &mut server_channel);
        assert_eq!(
            (
                acknowledgement.base_state,
                acknowledgement.target_state,
                acknowledgement.acknowledged_state,
            ),
            (u64::MAX, u64::MAX, u64::MAX)
        );
        assert!(completion.contains(&SessionAction::ShutdownComplete(
            ShutdownOutcome::PeerRequested
        )));
    }

    #[test]
    fn local_shutdown_has_a_bounded_acknowledgement_timeout() {
        let mut runtime = SessionRuntime::new(key(), time(0));
        runtime
            .request_shutdown(time(0))
            .expect("shutdown must start");
        runtime.poll(time(0)).expect("shutdown state must send");

        let completion = runtime
            .poll(time(super::SHUTDOWN_TIMEOUT_MILLISECONDS))
            .expect("shutdown timeout must be deterministic");
        assert!(completion.contains(&SessionAction::ShutdownComplete(ShutdownOutcome::TimedOut)));
        assert_eq!(runtime.milliseconds_until_next_poll(time(20_000)), u64::MAX);
    }

    fn non_diagnostic_actions(actions: Vec<SessionAction>) -> Vec<SessionAction> {
        actions
            .into_iter()
            .filter(|action| !matches!(action, SessionAction::Diagnostic(_)))
            .collect()
    }

    fn server_packet(acknowledged_state: u64, output: &[u8]) -> Vec<u8> {
        server_packet_with_echoed_timestamp(
            acknowledged_state,
            output,
            super::STANDARD_MOSH_NO_ECHO_TIMESTAMP,
        )
    }

    fn server_packet_with_echoed_timestamp(
        acknowledged_state: u64,
        output: &[u8],
        echoed_timestamp: u16,
    ) -> Vec<u8> {
        server_instruction_packet_with_echoed_timestamp(
            acknowledged_state,
            Instruction {
                bytes: Some(ByteRun {
                    value: output.to_vec(),
                }),
                viewport: None,
                marker: None,
                session_control: None,
            },
            echoed_timestamp,
        )
    }

    fn server_instruction_packet(acknowledged_state: u64, instruction: Instruction) -> Vec<u8> {
        server_instruction_packet_with_echoed_timestamp(
            acknowledged_state,
            instruction,
            super::STANDARD_MOSH_NO_ECHO_TIMESTAMP,
        )
    }

    fn server_instruction_packet_with_echoed_timestamp(
        acknowledged_state: u64,
        instruction: Instruction,
        echoed_timestamp: u16,
    ) -> Vec<u8> {
        let batch = InstructionBatch {
            instructions: vec![instruction],
        };
        let mut update = StateUpdate::new(0, 1, acknowledged_state);
        update.delta = batch.encode_bytes();
        let compressed = encode_compressed_update(&update).expect("state must compress");
        let fragment = Fragment::split(&compressed, 1, echoed_timestamp, 1)
            .expect("state must fragment")
            .remove(0);
        let mut channel = SecureChannel::new(PeerRole::Server, key());
        channel
            .seal_next(&fragment.encode())
            .expect("server packet must seal")
    }

    fn server_state_packet(base_state: u64, target_state: u64, acknowledged_state: u64) -> Vec<u8> {
        let update = StateUpdate::new(base_state, target_state, acknowledged_state);
        encoded_server_update(&update, target_state)
    }

    fn encoded_server_update(update: &StateUpdate, fragment_id: u64) -> Vec<u8> {
        let mut channel = SecureChannel::new(PeerRole::Server, key());
        encoded_server_update_with_channel(&mut channel, update, fragment_id)
    }

    fn encoded_server_update_with_channel(
        channel: &mut SecureChannel,
        update: &StateUpdate,
        fragment_id: u64,
    ) -> Vec<u8> {
        let compressed = encode_compressed_update(update).expect("state must compress");
        let fragment = Fragment::split(&compressed, 1, 0, fragment_id)
            .expect("state must fragment")
            .remove(0);
        channel
            .seal_next(&fragment.encode())
            .expect("server packet must seal")
    }

    fn decode_sent_update(
        actions: &[SessionAction],
        server_channel: &mut SecureChannel,
    ) -> StateUpdate {
        actions
            .iter()
            .filter_map(|action| match action {
                SessionAction::SendDatagram(packet) => Some(packet),
                _ => None,
            })
            .map(|packet| {
                let plaintext = server_channel
                    .open(packet)
                    .expect("client datagram must authenticate")
                    .plaintext;
                let fragment =
                    Fragment::parse(&plaintext).expect("datagram must contain a fragment");
                decode_compressed_update(&fragment.body).expect("state update must decode")
            })
            .last()
            .expect("actions must contain a datagram")
    }

    fn key() -> SessionKey {
        SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode")
    }

    const fn time(value: u64) -> MonotonicTime {
        MonotonicTime::from_milliseconds(value)
    }
}
