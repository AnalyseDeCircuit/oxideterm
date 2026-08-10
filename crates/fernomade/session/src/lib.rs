// SPDX-License-Identifier: Apache-2.0

//! Client-side protocol state independent of sockets and terminal adapters.

use fernomade_crypto::{OpenError, PeerRole, SealError, SecureChannel, SessionKey};
use fernomade_state::{RemoteTerminalState, StateError};
use fernomade_wire::{
    Fragment, FragmentAccumulator, FragmentError, Instruction, InstructionBatch, MessageError,
    StateUpdate, decode_compressed_update, encode_compressed_update,
};
use std::collections::{BTreeMap, VecDeque};

const NO_ECHO_TIMESTAMP: u16 = u16::MAX;
const MAXIMUM_REASSEMBLED_BYTES: usize = 16 * 1024 * 1024;
const MAXIMUM_ACTIVE_ASSEMBLIES: usize = 64;
const MAXIMUM_SENT_STATES: usize = 32;
const MAXIMUM_RECEIVED_STATES: usize = 1_024;
const REPLAY_WINDOW_BITS: u64 = 128;
const DEFAULT_REMOTE_COLUMNS: u16 = 80;
const DEFAULT_REMOTE_ROWS: u16 = 24;
const TIMESTAMP_HEADER_BYTES: usize = 4;
const MINIMUM_FRAGMENT_PLAINTEXT_BYTES: usize = 14;

/// Produces authenticated client updates and accepts authenticated server states.
pub struct ClientProtocol {
    secure_channel: SecureChannel,
    next_local_state: u64,
    latest_remote_state: u64,
    last_remote_base_state: u64,
    last_remote_target_state: u64,
    latest_peer_timestamp: Option<u16>,
    assemblies: BTreeMap<u64, FragmentAccumulator>,
    local_instructions: Vec<Instruction>,
    sent_states: VecDeque<SentState>,
    received_states: BTreeMap<u64, RemoteTerminalState>,
    delivered_remote_state: RemoteTerminalState,
    remote_throwaway_floor: u64,
    local_capabilities: Vec<u8>,
    remote_capabilities: Vec<u8>,
    replay_window: ReplayWindow,
}

impl ClientProtocol {
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn new(key: SessionKey) -> Self {
        let initial_remote_state =
            RemoteTerminalState::new(DEFAULT_REMOTE_COLUMNS, DEFAULT_REMOTE_ROWS)
                .expect("the default remote viewport is valid");
        Self {
            secure_channel: SecureChannel::new(PeerRole::Client, key),
            next_local_state: 1,
            latest_remote_state: 0,
            last_remote_base_state: 0,
            last_remote_target_state: 0,
            latest_peer_timestamp: None,
            assemblies: BTreeMap::new(),
            local_instructions: Vec::new(),
            sent_states: VecDeque::from([SentState {
                state_id: 0,
                instruction_end: 0,
            }]),
            received_states: BTreeMap::from([(0, initial_remote_state.clone())]),
            delivered_remote_state: initial_remote_state,
            remote_throwaway_floor: 0,
            local_capabilities: Vec::new(),
            remote_capabilities: Vec::new(),
            replay_window: ReplayWindow::default(),
        }
    }

    /// Builds the initial empty state-zero datagram used to associate a client endpoint.
    ///
    /// This packet does not advance local SSP state or create retransmission history.
    ///
    /// # Errors
    ///
    /// Returns an error if compression, fragmentation, or authenticated encryption fails.
    pub fn build_association(&mut self, sent_timestamp: u16) -> Result<Vec<Vec<u8>>, SessionError> {
        self.build_acknowledgement(sent_timestamp)
    }

    /// Builds an empty acknowledgement or heartbeat without advancing local SSP state.
    ///
    /// # Errors
    ///
    /// Returns an error if state history, compression, fragmentation, or encryption fails.
    pub fn build_acknowledgement(
        &mut self,
        sent_timestamp: u16,
    ) -> Result<Vec<Vec<u8>>, SessionError> {
        let local_state = self
            .sent_states
            .back()
            .ok_or(SessionError::StateHistoryUnavailable)?
            .state_id;
        let mut update = StateUpdate::new(local_state, local_state, self.latest_remote_state);
        update.discard_before = self
            .sent_states
            .front()
            .ok_or(SessionError::StateHistoryUnavailable)?
            .state_id;
        update.capabilities.clone_from(&self.local_capabilities);
        let compressed = encode_compressed_update(&update).map_err(SessionError::Message)?;
        seal_fragments(
            &mut self.secure_channel,
            &compressed,
            sent_timestamp,
            self.latest_peer_timestamp,
            local_state,
        )
    }

    /// Sets extension capability bytes advertised on subsequent updates.
    pub fn set_local_capabilities(&mut self, capabilities: Vec<u8>) {
        self.local_capabilities = capabilities;
    }

    /// Returns the latest non-empty capability advertisement from the peer.
    #[must_use]
    pub fn remote_capabilities(&self) -> &[u8] {
        &self.remote_capabilities
    }

    /// Reports whether both peers advertise the requested first-byte capability bit.
    #[must_use]
    pub fn has_negotiated_capability(&self, capability: u8) -> bool {
        self.local_capabilities
            .first()
            .zip(self.remote_capabilities.first())
            .is_some_and(|(local, remote)| local & remote & capability != 0)
    }

    /// Encodes one client state update and returns one or more UDP payloads.
    ///
    /// # Errors
    ///
    /// Returns an error for state-number exhaustion, compression, excessive
    /// fragmentation, or authenticated-encryption failure.
    pub fn build_update(
        &mut self,
        sent_timestamp: u16,
        instructions: &InstructionBatch,
    ) -> Result<Vec<Vec<u8>>, SessionError> {
        let target_state = self.next_local_state;
        if target_state == u64::MAX {
            return Err(SessionError::StateExhausted);
        }
        let packets = self.build_state_update(sent_timestamp, instructions, target_state)?;
        self.next_local_state = target_state + 1;
        Ok(packets)
    }

    /// Encodes the reserved SSP shutdown state with any final queued input.
    ///
    /// # Errors
    ///
    /// Returns an error if shutdown was already sent, state history is
    /// unavailable, or encoding and authentication fail.
    pub fn build_shutdown(
        &mut self,
        sent_timestamp: u16,
        instructions: &InstructionBatch,
    ) -> Result<Vec<Vec<u8>>, SessionError> {
        if self.local_shutdown_sent() {
            return Err(SessionError::ShutdownAlreadySent);
        }
        self.build_state_update(sent_timestamp, instructions, u64::MAX)
    }

    /// Re-sends the reserved local shutdown state to acknowledge a peer
    /// shutdown request after the peer has already acknowledged local state.
    ///
    /// # Errors
    ///
    /// Returns an error unless the acknowledged local shutdown state is the
    /// only retained state, or if encoding and authentication fail.
    pub fn build_shutdown_ack(
        &mut self,
        sent_timestamp: u16,
    ) -> Result<Vec<Vec<u8>>, SessionError> {
        if !self.local_shutdown_acknowledged() {
            return Err(SessionError::ShutdownNotAcknowledged);
        }
        let mut update = StateUpdate::new(u64::MAX, u64::MAX, self.latest_remote_state);
        update.discard_before = u64::MAX;
        update.capabilities.clone_from(&self.local_capabilities);
        update.delta = InstructionBatch {
            instructions: Vec::new(),
        }
        .encode_bytes();
        let compressed = encode_compressed_update(&update).map_err(SessionError::Message)?;
        seal_fragments(
            &mut self.secure_channel,
            &compressed,
            sent_timestamp,
            self.latest_peer_timestamp,
            u64::MAX,
        )
    }

    fn build_state_update(
        &mut self,
        sent_timestamp: u16,
        instructions: &InstructionBatch,
        target_state: u64,
    ) -> Result<Vec<Vec<u8>>, SessionError> {
        let base_state = self
            .sent_states
            .back()
            .ok_or(SessionError::StateHistoryUnavailable)?
            .state_id;
        let discard_before = self
            .sent_states
            .front()
            .ok_or(SessionError::StateHistoryUnavailable)?
            .state_id;
        let instruction_end = self
            .local_instructions
            .len()
            .checked_add(instructions.instructions.len())
            .ok_or(SessionError::StateExhausted)?;
        let mut update = StateUpdate::new(base_state, target_state, self.latest_remote_state);
        update.discard_before = discard_before;
        update.delta = instructions.encode_bytes();
        update.capabilities.clone_from(&self.local_capabilities);
        let compressed = encode_compressed_update(&update).map_err(SessionError::Message)?;
        let packets = seal_fragments(
            &mut self.secure_channel,
            &compressed,
            sent_timestamp,
            self.latest_peer_timestamp,
            target_state,
        )?;
        self.local_instructions
            .extend(instructions.instructions.iter().cloned());
        self.sent_states.push_back(SentState {
            state_id: target_state,
            instruction_end,
        });
        self.limit_sent_states();
        Ok(packets)
    }

    /// Rebuilds all unacknowledged input from the confirmed state and seals it
    /// with fresh packet counters after a timeout.
    ///
    /// # Errors
    ///
    /// Returns an error when no state awaits acknowledgement or when fragment
    /// encoding or authenticated encryption fails.
    pub fn retransmit_pending(
        &mut self,
        sent_timestamp: u16,
    ) -> Result<Vec<Vec<u8>>, SessionError> {
        if !self.has_pending_update() {
            return Err(SessionError::NoPendingUpdate);
        }
        let confirmed = self
            .sent_states
            .front()
            .ok_or(SessionError::StateHistoryUnavailable)?;
        let latest = self
            .sent_states
            .back()
            .ok_or(SessionError::StateHistoryUnavailable)?;
        let instructions = InstructionBatch {
            instructions: self.local_instructions
                [confirmed.instruction_end..latest.instruction_end]
                .to_vec(),
        };
        let mut update = StateUpdate::new(
            confirmed.state_id,
            latest.state_id,
            self.latest_remote_state,
        );
        update.discard_before = confirmed.state_id;
        update.delta = instructions.encode_bytes();
        update.capabilities.clone_from(&self.local_capabilities);
        let compressed = encode_compressed_update(&update).map_err(SessionError::Message)?;
        seal_fragments(
            &mut self.secure_channel,
            &compressed,
            sent_timestamp,
            self.latest_peer_timestamp,
            latest.state_id,
        )
    }

    /// Accepts one UDP payload and returns a complete server state when ready.
    ///
    /// # Errors
    ///
    /// Returns an error for failed authentication, invalid fragments, bounded
    /// reassembly failures or invalid messages.
    pub fn ingest(&mut self, packet: &[u8]) -> Result<Option<ReceivedState>, SessionError> {
        self.ingest_packet(packet).map(|receipt| receipt.state)
    }

    /// Accepts one UDP payload and reports every authenticated packet, including
    /// timestamp-only heartbeats and incomplete fragmented messages.
    ///
    /// # Errors
    ///
    /// Returns an error for failed authentication, replay, framing, bounded
    /// reassembly failures or invalid messages.
    pub fn ingest_packet(&mut self, packet: &[u8]) -> Result<PacketReceipt, SessionError> {
        let authenticated = self
            .secure_channel
            .open(packet)
            .map_err(SessionError::Open)?;
        if !self.replay_window.accept(authenticated.counter) {
            return Err(SessionError::ReplayDetected);
        }
        let timestamp_header = authenticated
            .plaintext
            .get(..TIMESTAMP_HEADER_BYTES)
            .ok_or(SessionError::Fragment(FragmentError::HeaderTooShort))?;
        let sent_timestamp = u16::from_be_bytes([timestamp_header[0], timestamp_header[1]]);
        let echoed_timestamp = u16::from_be_bytes([timestamp_header[2], timestamp_header[3]]);
        self.latest_peer_timestamp = Some(sent_timestamp);
        if authenticated.plaintext.len() < MINIMUM_FRAGMENT_PLAINTEXT_BYTES {
            return Ok(PacketReceipt {
                packet_counter: authenticated.counter,
                sent_timestamp,
                echoed_timestamp,
                state: None,
            });
        }
        let fragment = Fragment::parse(&authenticated.plaintext).map_err(SessionError::Fragment)?;
        let header = fragment.header;
        if !self.assemblies.contains_key(&header.state_id)
            && self.assemblies.len() >= MAXIMUM_ACTIVE_ASSEMBLIES
        {
            return Err(SessionError::TooManyActiveAssemblies);
        }
        let assembly = self.assemblies.entry(header.state_id).or_insert_with(|| {
            FragmentAccumulator::new(header.state_id, MAXIMUM_REASSEMBLED_BYTES)
        });
        let Some(compressed) = assembly.push(fragment).map_err(SessionError::Fragment)? else {
            return Ok(PacketReceipt {
                packet_counter: authenticated.counter,
                sent_timestamp,
                echoed_timestamp,
                state: None,
            });
        };
        self.assemblies.remove(&header.state_id);
        let update = decode_compressed_update(&compressed).map_err(SessionError::Message)?;
        let remote_capabilities = update.capabilities.clone();
        self.process_acknowledgement(update.acknowledged_state);
        let (advances_remote_state, delivered_update, raw_delta) =
            self.accept_remote_update(update)?;
        if !remote_capabilities.is_empty() {
            self.remote_capabilities = remote_capabilities;
        }
        let state = ReceivedState {
            packet_counter: authenticated.counter,
            sent_timestamp: header.sent_timestamp,
            echoed_timestamp: header.echoed_timestamp,
            advances_remote_state,
            raw_delta,
            update: delivered_update,
        };
        Ok(PacketReceipt {
            packet_counter: authenticated.counter,
            sent_timestamp,
            echoed_timestamp,
            state: Some(state),
        })
    }

    #[must_use]
    pub fn latest_remote_state(&self) -> u64 {
        self.latest_remote_state
    }

    /// Returns the state number that will contain newly queued input.
    #[must_use]
    pub fn next_local_state(&self) -> u64 {
        self.next_local_state
    }

    /// Returns the newest local state represented by pending transport history.
    #[must_use]
    pub fn latest_local_state(&self) -> u64 {
        self.sent_states.back().map_or(0, |state| state.state_id)
    }

    /// Returns content-free SSP state numbers for host transport diagnostics.
    #[must_use]
    pub fn transport_state_numbers(&self) -> TransportStateNumbers {
        TransportStateNumbers {
            acked_by_remote: self.sent_states.front().map_or(0, |state| state.state_id),
            sent_num: self.latest_local_state(),
            last_recv_old_num: self.last_remote_base_state,
            last_recv_new_num: self.last_remote_target_state,
            throwaway_num: self.remote_throwaway_floor,
        }
    }

    /// Updates the blank remote baseline before the first server state arrives.
    pub fn set_initial_remote_viewport(&mut self, columns: u16, rows: u16) {
        if self.latest_remote_state != 0 || self.received_states.len() != 1 {
            return;
        }
        let Ok(initial_state) = RemoteTerminalState::new(columns.max(1), rows.max(1)) else {
            return;
        };
        self.received_states.insert(0, initial_state.clone());
        self.delivered_remote_state = initial_state;
    }

    #[must_use]
    pub fn has_pending_update(&self) -> bool {
        self.sent_states.len() > 1
    }

    #[must_use]
    pub fn local_shutdown_sent(&self) -> bool {
        self.sent_states
            .back()
            .is_some_and(|state| state.state_id == u64::MAX)
    }

    #[must_use]
    pub fn local_shutdown_acknowledged(&self) -> bool {
        self.sent_states
            .front()
            .is_some_and(|state| state.state_id == u64::MAX)
    }

    #[must_use]
    pub const fn remote_shutdown_received(&self) -> bool {
        self.latest_remote_state == u64::MAX
    }

    fn process_acknowledgement(&mut self, acknowledged_state: u64) {
        let Some(acknowledged_index) = self
            .sent_states
            .iter()
            .position(|state| state.state_id == acknowledged_state)
        else {
            return;
        };
        self.sent_states.drain(..acknowledged_index);

        let confirmed_instruction_end = self
            .sent_states
            .front()
            .expect("an exact acknowledgement leaves its state in history")
            .instruction_end;
        if confirmed_instruction_end == 0 {
            return;
        }
        self.local_instructions.drain(..confirmed_instruction_end);
        for state in &mut self.sent_states {
            state.instruction_end -= confirmed_instruction_end;
        }
    }

    fn limit_sent_states(&mut self) {
        while self.sent_states.len() > MAXIMUM_SENT_STATES {
            // Match Mosh's bounded-history policy by removing a middle state
            // while preserving the confirmed front and the newest states.
            let removal_index = self.sent_states.len() - 16;
            self.sent_states.remove(removal_index);
        }
    }

    fn accept_remote_update(
        &mut self,
        update: StateUpdate,
    ) -> Result<(bool, StateUpdate, Vec<u8>), SessionError> {
        let raw_delta = update.delta.clone();
        if self.received_states.contains_key(&update.target_state) {
            return Ok((false, update, raw_delta));
        }
        if update.discard_before > update.base_state || update.target_state <= update.base_state {
            return Err(SessionError::InvalidStateTransition);
        }
        let Some(mut target_state) = self.received_states.get(&update.base_state).cloned() else {
            return Ok((false, update, raw_delta));
        };
        let instructions = update
            .decode_instructions()
            .map_err(SessionError::Message)?;
        target_state
            .apply(&instructions)
            .map_err(SessionError::State)?;

        let throwaway_floor = self.remote_throwaway_floor.max(update.discard_before);
        self.received_states
            .retain(|state_id, _| *state_id >= throwaway_floor);
        self.remote_throwaway_floor = throwaway_floor;
        if self.received_states.len() >= MAXIMUM_RECEIVED_STATES {
            return Err(SessionError::TooManyRemoteStates);
        }
        self.received_states
            .insert(update.target_state, target_state.clone());
        self.last_remote_base_state = update.base_state;
        self.last_remote_target_state = update.target_state;

        if update.target_state <= self.latest_remote_state {
            return Ok((false, update, raw_delta));
        }
        let Ok(diff) = target_state.diff_from(&self.delivered_remote_state) else {
            return Ok((false, update, raw_delta));
        };
        let mut delivered_update = update;
        delivered_update.delta = diff.into_instructions().encode_bytes();
        self.latest_remote_state = delivered_update.target_state;
        self.delivered_remote_state = target_state;
        Ok((true, delivered_update, raw_delta))
    }
}

struct SentState {
    state_id: u64,
    instruction_end: usize,
}

/// Content-free SSP state numbers observed by the client protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportStateNumbers {
    pub acked_by_remote: u64,
    pub sent_num: u64,
    pub last_recv_old_num: u64,
    pub last_recv_new_num: u64,
    pub throwaway_num: u64,
}

#[derive(Default)]
struct ReplayWindow {
    highest: Option<u64>,
    seen: u128,
}

impl ReplayWindow {
    fn accept(&mut self, counter: u64) -> bool {
        let Some(highest) = self.highest else {
            self.highest = Some(counter);
            self.seen = 1;
            return true;
        };
        if counter > highest {
            let advance = counter - highest;
            self.seen = if advance >= REPLAY_WINDOW_BITS {
                1
            } else {
                (self.seen << advance) | 1
            };
            self.highest = Some(counter);
            return true;
        }

        let age = highest - counter;
        if age >= REPLAY_WINDOW_BITS {
            return false;
        }
        let bit = 1_u128 << age;
        if self.seen & bit != 0 {
            return false;
        }
        self.seen |= bit;
        true
    }
}

fn seal_fragments(
    channel: &mut SecureChannel,
    compressed: &[u8],
    sent_timestamp: u16,
    latest_peer_timestamp: Option<u16>,
    state_id: u64,
) -> Result<Vec<Vec<u8>>, SessionError> {
    let fragments = Fragment::split(
        compressed,
        sent_timestamp,
        latest_peer_timestamp.unwrap_or(NO_ECHO_TIMESTAMP),
        state_id,
    )
    .map_err(SessionError::Fragment)?;
    fragments
        .into_iter()
        .map(|fragment| {
            channel
                .seal_next(&fragment.encode())
                .map_err(SessionError::Seal)
        })
        .collect()
}

/// Metadata observed from every authenticated server packet.
#[derive(PartialEq)]
pub struct PacketReceipt {
    pub packet_counter: u64,
    pub sent_timestamp: u16,
    pub echoed_timestamp: u16,
    pub state: Option<ReceivedState>,
}

impl std::fmt::Debug for PacketReceipt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PacketReceipt")
            .field("packet_counter", &self.packet_counter)
            .field("sent_timestamp", &self.sent_timestamp)
            .field("echoed_timestamp", &self.echoed_timestamp)
            .field("has_state", &self.state.is_some())
            .finish()
    }
}

#[derive(PartialEq)]
pub struct ReceivedState {
    pub packet_counter: u64,
    pub sent_timestamp: u16,
    pub echoed_timestamp: u16,
    pub advances_remote_state: bool,
    /// Original encoded instruction bytes from the accepted peer update.
    pub raw_delta: Vec<u8>,
    pub update: StateUpdate,
}

impl std::fmt::Debug for ReceivedState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReceivedState")
            .field("packet_counter", &self.packet_counter)
            .field("sent_timestamp", &self.sent_timestamp)
            .field("echoed_timestamp", &self.echoed_timestamp)
            .field("advances_remote_state", &self.advances_remote_state)
            .field("base_state", &self.update.base_state)
            .field("target_state", &self.update.target_state)
            .field("acknowledged_state", &self.update.acknowledged_state)
            .field("discard_before", &self.update.discard_before)
            .field("raw_delta_bytes", &self.raw_delta.len())
            .field("delta_bytes", &self.update.delta.len())
            .finish()
    }
}

#[derive(Debug)]
pub enum SessionError {
    StateExhausted,
    ShutdownAlreadySent,
    ShutdownNotAcknowledged,
    StateHistoryUnavailable,
    UpdateAwaitingAcknowledgement,
    NoPendingUpdate,
    ReplayDetected,
    TooManyActiveAssemblies,
    TooManyRemoteStates,
    InvalidStateTransition,
    State(StateError),
    Seal(SealError),
    Open(OpenError),
    Fragment(FragmentError),
    Message(MessageError),
}

#[cfg(test)]
mod tests {
    use super::{ClientProtocol, ReceivedState, ReplayWindow};
    use fernomade_crypto::{PeerRole, SecureChannel, SessionKey};
    use fernomade_state::RemoteTerminalState;
    use fernomade_wire::{
        ByteRun, Fragment, Instruction, InstructionBatch, StateUpdate, decode_compressed_update,
        encode_compressed_update,
    };

    const SYNTHETIC_KEY: &str = "AAECAwQFBgcICQoLDA0ODw";

    #[test]
    fn received_state_debug_redacts_terminal_delta() {
        let mut update = StateUpdate::new(1, 2, 3);
        update.delta = b"terminal-delta-sentinel".to_vec();
        let state = ReceivedState {
            packet_counter: 4,
            sent_timestamp: 5,
            echoed_timestamp: 6,
            advances_remote_state: true,
            raw_delta: b"raw-delta-sentinel".to_vec(),
            update,
        };
        let output = format!("{state:?}");
        assert!(!output.contains("terminal-delta-sentinel"));
        assert!(!output.contains("raw-delta-sentinel"));
        assert!(output.contains("delta_bytes: 23"));
    }

    #[test]
    fn authenticated_heartbeat_is_observed_and_echoed_without_a_state() {
        let mut client = ClientProtocol::new(key());
        let mut server = SecureChannel::new(PeerRole::Server, key());
        let heartbeat = server
            .seal_next(&[0x12, 0x34, 0x43, 0x21])
            .expect("heartbeat must seal");

        let receipt = client
            .ingest_packet(&heartbeat)
            .expect("heartbeat must authenticate");
        assert_eq!(receipt.sent_timestamp, 0x1234);
        assert_eq!(receipt.echoed_timestamp, 0x4321);
        assert!(receipt.state.is_none());
        assert!(!format!("{receipt:?}").contains("payload"));

        let acknowledgement = client
            .build_acknowledgement(0x5678)
            .expect("acknowledgement must encode");
        let plaintext = SecureChannel::new(PeerRole::Server, key())
            .open(&acknowledgement[0])
            .expect("acknowledgement must authenticate")
            .plaintext;
        let fragment = Fragment::parse(&plaintext).expect("acknowledgement must contain a state");
        assert_eq!(fragment.header.echoed_timestamp, 0x1234);
    }

    #[test]
    fn initial_update_matches_observed_state_shape() {
        let mut client = ClientProtocol::new(
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let instructions = InstructionBatch {
            instructions: vec![Instruction {
                bytes: Some(ByteRun {
                    value: b"exit\n".to_vec(),
                }),
                viewport: None,
                marker: None,
                session_control: None,
            }],
        };
        let packets = client
            .build_update(0x1234, &instructions)
            .expect("initial update must encode");
        assert_eq!(packets.len(), 1);

        let server_channel = SecureChannel::new(
            PeerRole::Server,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let plaintext = server_channel
            .open(&packets[0])
            .expect("server side must authenticate")
            .plaintext;
        let fragment = Fragment::parse(&plaintext).expect("fragment must parse");
        let update = decode_compressed_update(&fragment.body).expect("state must decode");

        assert_eq!((update.base_state, update.target_state), (0, 1));
        assert_eq!(update.acknowledged_state, 0);
        assert_eq!(
            update
                .decode_instructions()
                .expect("instructions must decode")
                .instructions[0]
                .bytes
                .as_ref()
                .map(|bytes| bytes.value.as_slice()),
            Some(b"exit\n".as_slice())
        );
    }

    #[test]
    fn retransmission_uses_new_packet_counter_and_clears_on_acknowledgement() {
        let mut client = ClientProtocol::new(
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let empty = InstructionBatch {
            instructions: Vec::new(),
        };
        let first = client
            .build_update(1, &empty)
            .expect("initial update must encode");
        let retransmitted = client
            .retransmit_pending(2)
            .expect("pending update must retransmit");
        assert_eq!(&first[0][..8], &[0; 8]);
        assert_eq!(&retransmitted[0][..8], &[0, 0, 0, 0, 0, 0, 0, 1]);

        let mut server_channel = SecureChannel::new(
            PeerRole::Server,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let acknowledgement = StateUpdate::new(0, 1, 1);
        let compressed =
            encode_compressed_update(&acknowledgement).expect("acknowledgement must compress");
        let fragment = Fragment::split(&compressed, 3, 2, 1)
            .expect("acknowledgement must fragment")
            .remove(0);
        let packet = server_channel
            .seal_next(&fragment.encode())
            .expect("server packet must seal");

        let received = client
            .ingest(&packet)
            .expect("acknowledgement must open")
            .expect("acknowledgement must complete");
        assert!(received.advances_remote_state);
        assert!(!client.has_pending_update());

        let retransmitted_packet = server_channel
            .seal_next(&fragment.encode())
            .expect("server retransmission must seal under a fresh counter");
        let retransmitted = client
            .ingest(&retransmitted_packet)
            .expect("server retransmission must authenticate")
            .expect("server retransmission must complete");
        assert!(!retransmitted.advances_remote_state);
        assert_eq!(client.latest_remote_state(), 1);
    }

    #[test]
    fn multiple_unacknowledged_states_retransmit_from_confirmed_front() {
        let mut client = ClientProtocol::new(key());
        let first = input_batch(b"first");
        let second = input_batch(b"second");
        let third = input_batch(b"third");

        let first_update = decode_client_update(
            &client
                .build_update(1, &first)
                .expect("first state must encode")[0],
        );
        let second_update = decode_client_update(
            &client
                .build_update(2, &second)
                .expect("second state must encode")[0],
        );
        let third_update = decode_client_update(
            &client
                .build_update(3, &third)
                .expect("third state must encode")[0],
        );

        assert_eq!((first_update.base_state, first_update.target_state), (0, 1));
        assert_eq!(
            (second_update.base_state, second_update.target_state),
            (1, 2)
        );
        assert_eq!((third_update.base_state, third_update.target_state), (2, 3));
        assert_eq!(third_update.discard_before, 0);

        let retransmitted = client
            .retransmit_pending(4)
            .expect("all pending input must retransmit");
        let retransmitted_update = decode_client_update(&retransmitted[0]);
        assert_eq!(
            (
                retransmitted_update.base_state,
                retransmitted_update.target_state,
                retransmitted_update.discard_before,
            ),
            (0, 3, 0)
        );
        let values = retransmitted_update
            .decode_instructions()
            .expect("retransmitted instructions must decode")
            .instructions
            .into_iter()
            .map(|instruction| instruction.bytes.expect("input bytes must exist").value)
            .collect::<Vec<_>>();
        assert_eq!(
            values,
            [b"first".to_vec(), b"second".to_vec(), b"third".to_vec()]
        );
    }

    #[test]
    fn acknowledgements_require_an_exact_retained_state() {
        let mut client = ClientProtocol::new(key());
        client
            .build_update(1, &input_batch(b"first"))
            .expect("first state must encode");
        client
            .build_update(2, &input_batch(b"second"))
            .expect("second state must encode");
        let mut server = SecureChannel::new(PeerRole::Server, key());

        let unknown_update = StateUpdate::new(0, 1, 99);
        let unknown_ack = server_update_packet(&mut server, &unknown_update, 1);
        client
            .ingest(&unknown_ack)
            .expect("unknown acknowledgement packet must open");
        assert!(client.has_pending_update());
        assert_eq!(
            client
                .sent_states
                .front()
                .expect("front must exist")
                .state_id,
            0
        );

        let exact_update = StateUpdate::new(1, 2, 1);
        let exact_ack = server_update_packet(&mut server, &exact_update, 2);
        client
            .ingest(&exact_ack)
            .expect("exact acknowledgement packet must open");
        assert_eq!(
            client
                .sent_states
                .front()
                .expect("front must exist")
                .state_id,
            1
        );
        let retransmitted = decode_client_update(
            &client
                .retransmit_pending(3)
                .expect("second state must remain pending")[0],
        );
        assert_eq!(
            (
                retransmitted.base_state,
                retransmitted.target_state,
                retransmitted.discard_before,
            ),
            (1, 2, 1)
        );
        assert_eq!(
            retransmitted
                .decode_instructions()
                .expect("remaining suffix must decode")
                .instructions[0]
                .bytes
                .as_ref()
                .expect("remaining input must exist")
                .value,
            b"second"
        );

        let latest_update = StateUpdate::new(2, 3, 2);
        let latest_ack = server_update_packet(&mut server, &latest_update, 3);
        client
            .ingest(&latest_ack)
            .expect("latest acknowledgement packet must open");
        assert!(!client.has_pending_update());
        assert_eq!(client.local_instructions.len(), 0);
    }

    #[test]
    fn sent_history_is_bounded_without_losing_front_or_latest() {
        let mut client = ClientProtocol::new(key());
        let empty = InstructionBatch {
            instructions: Vec::new(),
        };
        for timestamp in 1..=40 {
            client
                .build_update(timestamp, &empty)
                .expect("bounded state must encode");
        }

        assert_eq!(client.sent_states.len(), super::MAXIMUM_SENT_STATES);
        assert_eq!(
            client
                .sent_states
                .front()
                .expect("front must exist")
                .state_id,
            0
        );
        assert_eq!(
            client.sent_states.back().expect("back must exist").state_id,
            40
        );
    }

    #[test]
    fn maximum_state_number_is_reserved_as_a_sentinel() {
        let mut client = ClientProtocol::new(key());
        client.next_local_state = u64::MAX - 1;
        let empty = InstructionBatch {
            instructions: Vec::new(),
        };

        client
            .build_update(1, &empty)
            .expect("last ordinary state must encode");
        assert_eq!(client.next_local_state, u64::MAX);
        assert!(matches!(
            client.build_update(2, &empty),
            Err(super::SessionError::StateExhausted)
        ));
        assert_eq!(
            client.sent_states.back().expect("back must exist").state_id,
            u64::MAX - 1
        );
    }

    #[test]
    fn shutdown_state_is_retransmitted_until_exactly_acknowledged() {
        let mut client = ClientProtocol::new(key());
        let mut server = SecureChannel::new(PeerRole::Server, key());
        let final_input = input_batch(b"final");

        let packets = client
            .build_shutdown(1, &final_input)
            .expect("shutdown state must encode");
        let shutdown = decode_client_update(&packets[0]);
        assert_eq!((shutdown.base_state, shutdown.target_state), (0, u64::MAX));
        assert!(client.local_shutdown_sent());
        assert!(!client.local_shutdown_acknowledged());

        let retransmitted = client
            .retransmit_pending(2)
            .expect("shutdown state must retransmit");
        assert_eq!(
            decode_client_update(&retransmitted[0]).target_state,
            u64::MAX
        );

        let acknowledgement = StateUpdate::new(0, 1, u64::MAX);
        client
            .ingest(&server_update_packet(&mut server, &acknowledgement, 1))
            .expect("shutdown acknowledgement must authenticate");
        assert!(client.local_shutdown_acknowledged());
        assert!(!client.has_pending_update());
    }

    #[test]
    fn accepts_fragment_identifier_independent_of_target_state() {
        let mut client = ClientProtocol::new(key());
        let mut server = SecureChannel::new(PeerRole::Server, key());
        let update = StateUpdate::new(0, 1, 0);
        let packet = server_update_packet(&mut server, &update, 2);

        let received = client
            .ingest(&packet)
            .expect("independent fragment identifier must be accepted")
            .expect("complete update must be delivered");
        assert_eq!(received.update.target_state, 1);
        assert_eq!(client.latest_remote_state(), 1);
    }

    #[test]
    fn replay_window_accepts_reordering_once() {
        let mut window = ReplayWindow::default();

        assert!(window.accept(10));
        assert!(window.accept(12));
        assert!(window.accept(11));
        assert!(!window.accept(11));
        assert!(!window.accept(10));
        assert!(window.accept(140));
        assert!(!window.accept(12));
    }

    #[test]
    fn fragmented_server_state_converges_after_loss_and_reordering() {
        let mut client = ClientProtocol::new(
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let empty = InstructionBatch {
            instructions: Vec::new(),
        };
        let first_client_packets = client
            .build_update(1, &empty)
            .expect("initial update must encode");
        assert_eq!(first_client_packets.len(), 1);

        // Dropping the initial datagram must leave the exact logical update
        // available for resealing under a fresh packet counter.
        let retransmitted = client
            .retransmit_pending(2)
            .expect("dropped update must remain pending");
        let server_reader = SecureChannel::new(
            PeerRole::Server,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let retransmitted_plaintext = server_reader
            .open(&retransmitted[0])
            .expect("retransmission must authenticate")
            .plaintext;
        let retransmitted_update = decode_compressed_update(
            &Fragment::parse(&retransmitted_plaintext)
                .expect("retransmission must contain a fragment")
                .body,
        )
        .expect("retransmission must preserve the update");
        assert_eq!(
            (
                retransmitted_update.base_state,
                retransmitted_update.target_state
            ),
            (0, 1)
        );

        let mut pseudo_random_state = 0x1234_5678_u32;
        let terminal_bytes = (0..3_000)
            .map(|_| {
                pseudo_random_state ^= pseudo_random_state << 13;
                pseudo_random_state ^= pseudo_random_state >> 17;
                pseudo_random_state ^= pseudo_random_state << 5;
                pseudo_random_state.to_le_bytes()[0]
            })
            .collect::<Vec<_>>();
        let instructions = InstructionBatch {
            instructions: vec![Instruction {
                bytes: Some(ByteRun {
                    value: terminal_bytes.clone(),
                }),
                viewport: None,
                marker: None,
                session_control: None,
            }],
        };
        let mut server_update = StateUpdate::new(0, 2, 1);
        server_update.delta = instructions.encode_bytes();
        let compressed =
            encode_compressed_update(&server_update).expect("server state must compress");
        let fragments = Fragment::split(&compressed, 3, 2, 2).expect("server state must fragment");
        assert!(fragments.len() > 1);

        let mut server_writer = SecureChannel::new(
            PeerRole::Server,
            SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode"),
        );
        let mut packets = fragments
            .into_iter()
            .map(|fragment| {
                server_writer
                    .seal_next(&fragment.encode())
                    .expect("server fragment must seal")
            })
            .collect::<Vec<_>>();
        packets.reverse();

        let mut received = None;
        for packet in &packets {
            if let Some(state) = client.ingest(packet).expect("reordered packet must open") {
                received = Some(state);
            }
        }
        let received = received.expect("all reordered fragments must converge");
        assert_eq!(received.update.target_state, 2);
        assert_eq!(
            received
                .update
                .decode_instructions()
                .expect("instructions must decode")
                .instructions[0]
                .bytes
                .as_ref()
                .expect("terminal bytes must exist")
                .value,
            terminal_bytes
        );
        assert!(!client.has_pending_update());
        assert!(matches!(
            client.ingest(&packets[0]),
            Err(super::SessionError::ReplayDetected)
        ));
    }

    #[test]
    fn older_retained_remote_base_reconstructs_without_repeating_host_bytes() {
        let mut client = ClientProtocol::new(key());
        let mut server = SecureChannel::new(PeerRole::Server, key());

        let first = remote_update(0, 1, 0, b"A");
        let second = remote_update(1, 2, 0, b"B");
        let rebuilt = remote_update(1, 3, 1, b"BC");
        assert_eq!(
            received_bytes(&mut client, &server_update_packet(&mut server, &first, 1)),
            b"A"
        );
        assert_eq!(
            received_bytes(&mut client, &server_update_packet(&mut server, &second, 2)),
            b"B"
        );
        assert_eq!(
            received_bytes(&mut client, &server_update_packet(&mut server, &rebuilt, 3)),
            b"C"
        );
        assert_eq!(client.latest_remote_state(), 3);

        let discarded_base = remote_update(0, 4, 0, b"ignored");
        let packet = server_update_packet(&mut server, &discarded_base, 4);
        let received = client
            .ingest(&packet)
            .expect("missing base is ignored")
            .expect("complete packet is reported");
        assert!(!received.advances_remote_state);
        assert_eq!(client.latest_remote_state(), 3);
    }

    #[test]
    fn diverged_plain_text_remote_state_converges_at_a_parser_boundary() {
        let mut client = ClientProtocol::new(key());
        let mut server = SecureChannel::new(PeerRole::Server, key());
        let mut rendered = RemoteTerminalState::new(80, 24).expect("test viewport must be valid");

        let first = remote_update(0, 1, 0, b"A");
        let second = remote_update(1, 2, 0, b"B");
        let replacement = remote_update(1, 3, 1, b"C");
        for (update, fragment_state_id) in [(&first, 1), (&second, 2), (&replacement, 3)] {
            let bytes = received_bytes(
                &mut client,
                &server_update_packet(&mut server, update, fragment_state_id),
            );
            rendered
                .apply(&input_batch(&bytes))
                .expect("delivered diff must render");
        }

        assert_eq!(client.latest_remote_state(), 3);
        assert_eq!(rendered.screen().contents(), "AC");
    }

    fn key() -> SessionKey {
        SessionKey::decode(SYNTHETIC_KEY).expect("synthetic key must decode")
    }

    fn input_batch(value: &[u8]) -> InstructionBatch {
        InstructionBatch {
            instructions: vec![Instruction {
                bytes: Some(ByteRun {
                    value: value.to_vec(),
                }),
                viewport: None,
                marker: None,
                session_control: None,
            }],
        }
    }

    fn decode_client_update(packet: &[u8]) -> StateUpdate {
        let server = SecureChannel::new(PeerRole::Server, key());
        let plaintext = server
            .open(packet)
            .expect("client packet must authenticate")
            .plaintext;
        let fragment = Fragment::parse(&plaintext).expect("client packet must contain a fragment");
        assert!(fragment.header.is_final);
        decode_compressed_update(&fragment.body).expect("client state must decode")
    }

    fn server_update_packet(
        server: &mut SecureChannel,
        update: &StateUpdate,
        fragment_state_id: u64,
    ) -> Vec<u8> {
        let compressed = encode_compressed_update(update).expect("server state must compress");
        let fragment = Fragment::split(&compressed, 1, 0, fragment_state_id)
            .expect("server state must fragment")
            .remove(0);
        server
            .seal_next(&fragment.encode())
            .expect("server packet must seal")
    }

    fn remote_update(
        base_state: u64,
        target_state: u64,
        discard_before: u64,
        bytes: &[u8],
    ) -> StateUpdate {
        let mut update = StateUpdate::new(base_state, target_state, 0);
        update.discard_before = discard_before;
        update.delta = input_batch(bytes).encode_bytes();
        update
    }

    fn received_bytes(client: &mut ClientProtocol, packet: &[u8]) -> Vec<u8> {
        client
            .ingest(packet)
            .expect("server state must open")
            .expect("server state must complete")
            .update
            .decode_instructions()
            .expect("instructions must decode")
            .instructions
            .into_iter()
            .filter_map(|instruction| instruction.bytes)
            .flat_map(|bytes| bytes.value)
            .collect()
    }
}
