// SPDX-License-Identifier: Apache-2.0

//! Cloneable remote terminal state for reconstructing SSP updates from history.

use fernomade_wire::{
    ByteRun, Instruction, InstructionBatch, Marker, SessionControl, ViewportSize,
};
use std::fmt;
use std::sync::Arc;

/// Maximum `HostBytes` retained by one logical terminal history.
pub const MAX_RETAINED_HOST_BYTES: usize = 16 * 1024 * 1024;
/// Maximum parser-affecting operations retained by one logical terminal history.
pub const MAX_RETAINED_OPERATIONS: usize = 65_536;

/// A terminal-state operation retained so parser state can be reconstructed exactly.
#[derive(Clone, Debug, Eq, PartialEq)]
enum StateOperation {
    HostBytes(Vec<u8>),
    Resize { columns: u16, rows: u16 },
    SessionControl(SessionControl),
}

struct HistoryNode {
    parent: Option<Arc<Self>>,
    operation: StateOperation,
}

#[derive(Clone, Default)]
struct StateHistory {
    tail: Option<Arc<HistoryNode>>,
    operation_count: usize,
    retained_bytes: usize,
}

impl StateHistory {
    fn push(&mut self, operation: StateOperation) {
        self.tail = Some(Arc::new(HistoryNode {
            parent: self.tail.clone(),
            operation,
        }));
        self.operation_count += 1;
    }

    fn operations(&self) -> Vec<&StateOperation> {
        let mut operations = Vec::with_capacity(self.operation_count);
        let mut current = self.tail.as_deref();
        while let Some(node) = current {
            operations.push(&node.operation);
            current = node.parent.as_deref();
        }
        operations.reverse();
        operations
    }

    fn canonical_operations(&self) -> Vec<StateOperation> {
        let mut canonical = Vec::new();
        for operation in self.operations() {
            match (canonical.last_mut(), operation) {
                (Some(StateOperation::HostBytes(previous)), StateOperation::HostBytes(bytes)) => {
                    previous.extend_from_slice(bytes);
                }
                _ => canonical.push(operation.clone()),
            }
        }
        canonical
    }
}

/// A cloneable logical terminal state, including incomplete parser input.
pub struct RemoteTerminalState {
    initial_columns: u16,
    initial_rows: u16,
    parser: vt100::Parser,
    history: StateHistory,
    viewport: ViewportSize,
    echo_ack: Option<u64>,
}

impl RemoteTerminalState {
    /// Creates an empty state with a nonzero initial viewport.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::InvalidViewport`] for zero dimensions.
    pub fn new(columns: u16, rows: u16) -> Result<Self, StateError> {
        validate_viewport(u64::from(columns), u64::from(rows))?;
        Ok(Self {
            initial_columns: columns,
            initial_rows: rows,
            parser: vt100::Parser::new(rows, columns, 0),
            history: StateHistory::default(),
            viewport: ViewportSize {
                columns: u64::from(columns),
                rows: u64::from(rows),
            },
            echo_ack: None,
        })
    }

    /// Applies one decoded instruction batch in instruction order.
    ///
    /// The batch is validated before mutation, so an invalid viewport cannot
    /// leave a partially applied state.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid viewport, a history limit, or a
    /// decreasing echo acknowledgement.
    pub fn apply(&mut self, batch: &InstructionBatch) -> Result<(), StateError> {
        for instruction in &batch.instructions {
            if let Some(viewport) = instruction.viewport {
                validate_viewport(viewport.columns, viewport.rows)?;
            }
        }
        let mut echo_ack = self.echo_ack;
        for marker in batch
            .instructions
            .iter()
            .filter_map(|instruction| instruction.marker)
        {
            if echo_ack.is_some_and(|current| marker.value < current) {
                return Err(StateError::EchoAckRegression);
            }
            echo_ack = Some(marker.value);
        }

        if self.validate_history_growth(batch).is_err() {
            let (history, initial_columns, initial_rows) = self.compacted_history(batch)?;
            self.history = history;
            self.initial_columns = initial_columns;
            self.initial_rows = initial_rows;
        }

        for instruction in &batch.instructions {
            if let Some(bytes) = &instruction.bytes {
                self.apply_host_bytes(&bytes.value);
            }
            if let Some(viewport) = instruction.viewport {
                let columns =
                    u16::try_from(viewport.columns).map_err(|_| StateError::InvalidViewport)?;
                let rows = u16::try_from(viewport.rows).map_err(|_| StateError::InvalidViewport)?;
                self.apply_resize(columns, rows);
            }
            if let Some(marker) = instruction.marker {
                self.echo_ack = Some(marker.value);
            }
            if let Some(session_control) = &instruction.session_control {
                self.history
                    .push(StateOperation::SessionControl(session_control.clone()));
            }
        }
        Ok(())
    }

    /// Returns the modeled terminal screen.
    #[must_use]
    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Returns the latest viewport applied to the state.
    #[must_use]
    pub const fn viewport(&self) -> ViewportSize {
        self.viewport
    }

    /// Returns the latest server echo acknowledgement, if any.
    #[must_use]
    pub const fn echo_ack(&self) -> Option<u64> {
        self.echo_ack
    }

    /// Returns the number of `HostBytes` retained for exact parser replay.
    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.history.retained_bytes
    }

    /// Returns the number of parser-affecting operations in this history.
    #[must_use]
    pub const fn operation_count(&self) -> usize {
        self.history.operation_count
    }

    /// Produces only effects not already present in `delivered`.
    ///
    /// SSP reconstruction normally yields a target whose operation history
    /// extends the last delivered state, even when it was rebuilt from an older
    /// base. A diverged history can use a screen diff only when both parsers are
    /// at a ground boundary and their histories contain no state that a screen
    /// diff cannot reconstruct. Otherwise the conservative prefix rule remains
    /// in effect.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::DivergedHistory`] when histories diverge outside
    /// the conservative screen-diff boundary described above.
    pub fn diff_from(&self, delivered: &Self) -> Result<StateDiff, StateError> {
        let target_operations = self.history.canonical_operations();
        let delivered_operations = delivered.history.canonical_operations();
        let mut instructions = match history_suffix_start(&target_operations, &delivered_operations)
        {
            Ok((suffix_start, first_byte_offset)) => target_operations[suffix_start..]
                .iter()
                .enumerate()
                .filter_map(|(index, operation)| match operation {
                    StateOperation::HostBytes(value) => {
                        let offset = if index == 0 { first_byte_offset } else { 0 };
                        let value = value[offset..].to_vec();
                        (!value.is_empty()).then_some(host_bytes_instruction(value))
                    }
                    StateOperation::Resize { columns, rows } => {
                        Some(viewport_instruction(*columns, *rows))
                    }
                    StateOperation::SessionControl(control) => {
                        Some(session_control_instruction(control.clone()))
                    }
                })
                .collect::<Vec<_>>(),
            Err(StateError::DivergedHistory)
                if self.viewport == delivered.viewport
                    && diverged_histories_are_screen_diff_safe(
                        &target_operations,
                        &delivered_operations,
                    ) =>
            {
                let value = self.screen().state_diff(delivered.screen());
                (!value.is_empty())
                    .then(|| host_bytes_instruction(value))
                    .into_iter()
                    .collect()
            }
            Err(error) => return Err(error),
        };
        if self.echo_ack != delivered.echo_ack {
            if let Some(value) = self.echo_ack {
                instructions.insert(
                    0,
                    Instruction {
                        bytes: None,
                        viewport: None,
                        marker: Some(Marker { value }),
                        session_control: None,
                    },
                );
            }
        }
        Ok(StateDiff {
            batch: InstructionBatch { instructions },
        })
    }

    fn apply_host_bytes(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
        if bytes.is_empty() {
            return;
        }
        self.history.retained_bytes += bytes.len();
        self.history.push(StateOperation::HostBytes(bytes.to_vec()));
    }

    fn apply_resize(&mut self, columns: u16, rows: u16) {
        if self.viewport.columns == u64::from(columns) && self.viewport.rows == u64::from(rows) {
            return;
        }
        self.parser.screen_mut().set_size(rows, columns);
        self.viewport = ViewportSize {
            columns: u64::from(columns),
            rows: u64::from(rows),
        };
        self.history.push(StateOperation::Resize { columns, rows });
    }

    fn validate_history_growth(&self, batch: &InstructionBatch) -> Result<(), StateError> {
        let (additional_bytes, additional_operations) = history_growth(batch, self.viewport)?;
        validate_history_size(
            self.history.retained_bytes,
            self.history.operation_count,
            additional_bytes,
            additional_operations,
        )
    }

    fn compacted_history(
        &self,
        batch: &InstructionBatch,
    ) -> Result<(StateHistory, u16, u16), StateError> {
        if !history_ends_at_ground(&self.history.canonical_operations()) {
            return Err(StateError::HistoryLimitExceeded);
        }
        let snapshot = self.screen().state_formatted();
        let (additional_bytes, additional_operations) = history_growth(batch, self.viewport)?;
        let snapshot_operations = usize::from(!snapshot.is_empty());
        validate_history_size(
            snapshot.len(),
            snapshot_operations,
            additional_bytes,
            additional_operations,
        )?;
        let tail = (!snapshot.is_empty()).then(|| {
            Arc::new(HistoryNode {
                parent: None,
                operation: StateOperation::HostBytes(snapshot.clone()),
            })
        });
        let columns =
            u16::try_from(self.viewport.columns).map_err(|_| StateError::InvalidViewport)?;
        let rows = u16::try_from(self.viewport.rows).map_err(|_| StateError::InvalidViewport)?;
        Ok((
            StateHistory {
                tail,
                operation_count: snapshot_operations,
                retained_bytes: snapshot.len(),
            },
            columns,
            rows,
        ))
    }
}

fn history_growth(
    batch: &InstructionBatch,
    initial_viewport: ViewportSize,
) -> Result<(usize, usize), StateError> {
    let mut additional_bytes = 0_usize;
    let mut additional_operations = 0_usize;
    let mut viewport = initial_viewport;
    for instruction in &batch.instructions {
        if let Some(bytes) = &instruction.bytes {
            additional_bytes = additional_bytes
                .checked_add(bytes.value.len())
                .ok_or(StateError::HistoryLimitExceeded)?;
            additional_operations += usize::from(!bytes.value.is_empty());
        }
        if let Some(next_viewport) = instruction.viewport {
            if viewport != next_viewport {
                additional_operations += 1;
                viewport = next_viewport;
            }
        }
        additional_operations += usize::from(instruction.session_control.is_some());
    }
    Ok((additional_bytes, additional_operations))
}

fn validate_history_size(
    retained_bytes: usize,
    operation_count: usize,
    additional_bytes: usize,
    additional_operations: usize,
) -> Result<(), StateError> {
    let retained_bytes = retained_bytes
        .checked_add(additional_bytes)
        .ok_or(StateError::HistoryLimitExceeded)?;
    let operation_count = operation_count
        .checked_add(additional_operations)
        .ok_or(StateError::HistoryLimitExceeded)?;
    if retained_bytes > MAX_RETAINED_HOST_BYTES || operation_count > MAX_RETAINED_OPERATIONS {
        return Err(StateError::HistoryLimitExceeded);
    }
    Ok(())
}

fn host_bytes_instruction(value: Vec<u8>) -> Instruction {
    Instruction {
        bytes: Some(ByteRun { value }),
        viewport: None,
        marker: None,
        session_control: None,
    }
}

fn viewport_instruction(columns: u16, rows: u16) -> Instruction {
    Instruction {
        bytes: None,
        viewport: Some(ViewportSize {
            columns: u64::from(columns),
            rows: u64::from(rows),
        }),
        marker: None,
        session_control: None,
    }
}

fn session_control_instruction(session_control: SessionControl) -> Instruction {
    Instruction {
        bytes: None,
        viewport: None,
        marker: None,
        session_control: Some(session_control),
    }
}

impl Clone for RemoteTerminalState {
    fn clone(&self) -> Self {
        let mut cloned = Self::new(self.initial_columns, self.initial_rows)
            .expect("an existing state always has a valid initial viewport");
        for operation in self.history.operations() {
            match operation {
                StateOperation::HostBytes(bytes) => cloned.parser.process(bytes),
                StateOperation::Resize { columns, rows } => {
                    cloned.parser.screen_mut().set_size(*rows, *columns);
                }
                StateOperation::SessionControl(_) => {}
            }
        }
        cloned.history = self.history.clone();
        cloned.viewport = self.viewport;
        cloned.echo_ack = self.echo_ack;
        cloned
    }
}

/// Effects required to advance a host from one delivered state to another.
#[derive(Clone, PartialEq)]
pub struct StateDiff {
    batch: InstructionBatch,
}

impl fmt::Debug for StateDiff {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let host_bytes = self
            .batch
            .instructions
            .iter()
            .filter_map(|instruction| instruction.bytes.as_ref())
            .map(|bytes| bytes.value.len())
            .sum::<usize>();
        formatter
            .debug_struct("StateDiff")
            .field("instructions", &self.batch.instructions.len())
            .field("host_bytes", &host_bytes)
            .finish()
    }
}

impl StateDiff {
    /// Borrows the ordered `HostBytes` and `EchoAck` instructions.
    #[must_use]
    pub const fn instructions(&self) -> &InstructionBatch {
        &self.batch
    }

    /// Consumes the diff and returns its ordered instructions.
    #[must_use]
    pub fn into_instructions(self) -> InstructionBatch {
        self.batch
    }
}

/// Failure to construct or compare logical terminal states safely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateError {
    InvalidViewport,
    HistoryLimitExceeded,
    DivergedHistory,
    EchoAckRegression,
}

fn validate_viewport(columns: u64, rows: u64) -> Result<(), StateError> {
    if columns == 0 || rows == 0 || u16::try_from(columns).is_err() || u16::try_from(rows).is_err()
    {
        return Err(StateError::InvalidViewport);
    }
    Ok(())
}

fn history_suffix_start(
    target: &[StateOperation],
    delivered: &[StateOperation],
) -> Result<(usize, usize), StateError> {
    for (index, delivered_operation) in delivered.iter().enumerate() {
        let Some(target_operation) = target.get(index) else {
            return Err(StateError::DivergedHistory);
        };
        match (target_operation, delivered_operation) {
            (
                StateOperation::HostBytes(target_bytes),
                StateOperation::HostBytes(delivered_bytes),
            ) if target_bytes.starts_with(delivered_bytes) => {
                if target_bytes.len() != delivered_bytes.len() {
                    if index + 1 != delivered.len() {
                        return Err(StateError::DivergedHistory);
                    }
                    return Ok((index, delivered_bytes.len()));
                }
            }
            (target_operation, delivered_operation) if target_operation == delivered_operation => {}
            _ => return Err(StateError::DivergedHistory),
        }
    }
    Ok((delivered.len(), 0))
}

fn diverged_histories_are_screen_diff_safe(
    target: &[StateOperation],
    delivered: &[StateOperation],
) -> bool {
    let (common, target_tail, delivered_tail) = split_common_history(target, delivered);
    history_tail_is_screen_diff_safe(&common, &target_tail)
        && history_tail_is_screen_diff_safe(&common, &delivered_tail)
}

fn split_common_history(
    target: &[StateOperation],
    delivered: &[StateOperation],
) -> (
    Vec<StateOperation>,
    Vec<StateOperation>,
    Vec<StateOperation>,
) {
    let mut common = Vec::new();
    let mut index = 0;
    while let (Some(target_operation), Some(delivered_operation)) =
        (target.get(index), delivered.get(index))
    {
        if target_operation == delivered_operation {
            common.push(target_operation.clone());
            index += 1;
            continue;
        }
        if let (
            StateOperation::HostBytes(target_bytes),
            StateOperation::HostBytes(delivered_bytes),
        ) = (target_operation, delivered_operation)
        {
            let common_bytes = target_bytes
                .iter()
                .zip(delivered_bytes)
                .take_while(|(target_byte, delivered_byte)| target_byte == delivered_byte)
                .count();
            if common_bytes != 0 {
                common.push(StateOperation::HostBytes(
                    target_bytes[..common_bytes].to_vec(),
                ));
            }
            let mut target_tail = Vec::new();
            if common_bytes != target_bytes.len() {
                target_tail.push(StateOperation::HostBytes(
                    target_bytes[common_bytes..].to_vec(),
                ));
            }
            target_tail.extend_from_slice(&target[index + 1..]);
            let mut delivered_tail = Vec::new();
            if common_bytes != delivered_bytes.len() {
                delivered_tail.push(StateOperation::HostBytes(
                    delivered_bytes[common_bytes..].to_vec(),
                ));
            }
            delivered_tail.extend_from_slice(&delivered[index + 1..]);
            return (common, target_tail, delivered_tail);
        }
        break;
    }
    (
        common,
        target[index..].to_vec(),
        delivered[index..].to_vec(),
    )
}

fn history_tail_is_screen_diff_safe(common: &[StateOperation], tail: &[StateOperation]) -> bool {
    let mut parser = vte::Parser::new();
    let mut audit = ScreenDiffAudit::default();
    for operation in common {
        if let StateOperation::HostBytes(bytes) = operation {
            parser.advance(&mut audit, bytes);
        }
    }
    audit.begin_tail_audit();
    for operation in tail {
        match operation {
            StateOperation::HostBytes(bytes) => parser.advance(&mut audit, bytes),
            StateOperation::Resize { .. } => return false,
            StateOperation::SessionControl(_) => {}
        }
    }
    // A printable probe is dispatched unchanged only when the VTE parser is
    // in ground state with no incomplete UTF-8 code point.
    audit.begin_boundary_probe();
    parser.advance(&mut audit, b"~");
    audit.probe_is_ground()
}

fn history_ends_at_ground(operations: &[StateOperation]) -> bool {
    let mut parser = vte::Parser::new();
    let mut audit = ScreenDiffAudit::default();
    for operation in operations {
        if let StateOperation::HostBytes(bytes) = operation {
            parser.advance(&mut audit, bytes);
        }
    }
    audit.begin_boundary_probe();
    parser.advance(&mut audit, b"~");
    audit.probe_is_ground()
}

struct ScreenDiffAudit {
    reconstructible: bool,
    auditing: bool,
    probing: bool,
    probe_prints: usize,
    probe_other_actions: usize,
}

impl Default for ScreenDiffAudit {
    fn default() -> Self {
        Self {
            reconstructible: true,
            auditing: false,
            probing: false,
            probe_prints: 0,
            probe_other_actions: 0,
        }
    }
}

impl ScreenDiffAudit {
    fn begin_tail_audit(&mut self) {
        self.auditing = true;
        self.reconstructible = true;
    }

    fn begin_boundary_probe(&mut self) {
        self.probing = true;
        self.probe_prints = 0;
        self.probe_other_actions = 0;
    }

    const fn probe_is_ground(&self) -> bool {
        self.probe_prints == 1 && self.probe_other_actions == 0
    }

    fn record_other_action(&mut self) {
        if self.probing {
            self.probe_other_actions += 1;
        } else if self.auditing {
            self.reconstructible = false;
        }
    }
}

impl vte::Perform for ScreenDiffAudit {
    fn print(&mut self, character: char) {
        if self.probing {
            self.probe_prints += usize::from(character == '~');
            self.probe_other_actions += usize::from(character != '~');
        }
    }

    fn execute(&mut self, byte: u8) {
        if self.probing {
            self.probe_other_actions += 1;
        } else if self.auditing && !matches!(byte, b'\x08' | b'\x09' | b'\x0a' | b'\x0d') {
            self.reconstructible = false;
        }
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {
        self.record_other_action();
    }

    fn put(&mut self, _byte: u8) {
        self.record_other_action();
    }

    fn unhook(&mut self) {
        self.record_other_action();
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
        self.record_other_action();
    }

    fn csi_dispatch(
        &mut self,
        _params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
        self.record_other_action();
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {
        self.record_other_action();
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_RETAINED_HOST_BYTES, RemoteTerminalState, StateError};
    use fernomade_wire::{ByteRun, Instruction, InstructionBatch, Marker, ViewportSize};

    #[test]
    fn old_baseline_reconstruction_does_not_repeat_host_bytes() {
        let mut baseline = state();
        baseline.apply(&bytes_batch(b"A")).expect("A must apply");

        let mut delivered = baseline.clone();
        delivered.apply(&bytes_batch(b"B")).expect("B must apply");

        let mut rebuilt = baseline.clone();
        rebuilt.apply(&bytes_batch(b"BC")).expect("BC must apply");
        let diff = rebuilt
            .diff_from(&delivered)
            .expect("rebuilt history extends delivered history");
        assert_eq!(host_bytes(diff.instructions()), b"C");
        assert_eq!(rebuilt.screen().contents(), "ABC");
    }

    #[test]
    fn clone_preserves_incomplete_utf8_and_csi_input() {
        let mut utf8 = state();
        utf8.apply(&bytes_batch(&[0xe7, 0x95]))
            .expect("UTF-8 prefix must apply");
        let mut utf8_branch = utf8.clone();
        utf8_branch
            .apply(&bytes_batch(&[0x8c]))
            .expect("UTF-8 suffix must apply");
        assert_eq!(utf8_branch.screen().contents(), "界");

        let mut csi = state();
        csi.apply(&bytes_batch(b"\x1b[?"))
            .expect("CSI prefix must apply");
        let mut csi_branch = csi.clone();
        csi_branch
            .apply(&bytes_batch(b"1h"))
            .expect("CSI suffix must apply");
        assert!(csi_branch.screen().application_cursor());
    }

    #[test]
    fn clone_preserves_incomplete_osc_input() {
        let mut state = state();
        state
            .apply(&bytes_batch(b"\x1b]2;partial"))
            .expect("OSC prefix must apply");
        let mut branch = state.clone();
        branch
            .apply(&bytes_batch(b" title\x07visible"))
            .expect("OSC suffix must apply");
        assert_eq!(branch.screen().contents(), "visible");
    }

    #[test]
    fn viewport_modes_and_echo_ack_survive_cloning() {
        let mut state = state();
        state
            .apply(&InstructionBatch {
                instructions: vec![Instruction {
                    bytes: Some(ByteRun {
                        value: b"\x1b[?1;2004h".to_vec(),
                    }),
                    viewport: Some(ViewportSize {
                        columns: 100,
                        rows: 40,
                    }),
                    marker: Some(Marker { value: 23 }),
                    session_control: None,
                }],
            })
            .expect("combined instruction must apply");
        let cloned = state.clone();
        assert_eq!(cloned.viewport().columns, 100);
        assert_eq!(cloned.viewport().rows, 40);
        assert_eq!(cloned.echo_ack(), Some(23));
        assert!(cloned.screen().application_cursor());
        assert!(cloned.screen().bracketed_paste());

        let diff = cloned.diff_from(&state).expect("equal states must diff");
        assert!(diff.instructions().instructions.is_empty());
    }

    #[test]
    fn changed_echo_ack_is_emitted_without_repeating_host_bytes() {
        let mut delivered = state();
        delivered
            .apply(&bytes_batch(b"ready"))
            .expect("bytes apply");
        let mut target = delivered.clone();
        target
            .apply(&InstructionBatch {
                instructions: vec![Instruction {
                    bytes: None,
                    viewport: None,
                    marker: Some(Marker { value: 41 }),
                    session_control: None,
                }],
            })
            .expect("marker applies");
        let instructions = target
            .diff_from(&delivered)
            .expect("target extends delivered")
            .into_instructions()
            .instructions;
        assert_eq!(instructions.len(), 1);
        assert_eq!(instructions[0].marker, Some(Marker { value: 41 }));
        assert!(instructions[0].bytes.is_none());
    }

    #[test]
    fn state_diff_debug_redacts_host_bytes() {
        let delivered = state();
        let mut target = delivered.clone();
        target
            .apply(&bytes_batch(b"state-diff-sentinel"))
            .expect("bytes apply");
        let diff = target.diff_from(&delivered).expect("history extends");
        let output = format!("{diff:?}");
        assert!(!output.contains("state-diff-sentinel"));
        assert!(output.contains("host_bytes: 19"));
    }

    #[test]
    fn diverged_plain_text_converges_through_a_screen_diff() {
        let mut left = state();
        left.apply(&bytes_batch(b"left")).expect("left applies");
        let mut right = state();
        right.apply(&bytes_batch(b"right")).expect("right applies");

        let diff = right
            .diff_from(&left)
            .expect("ground-state text can use a screen diff");
        left.apply(diff.instructions())
            .expect("screen diff must apply to delivered state");
        assert_eq!(left.screen().contents(), right.screen().contents());
    }

    #[test]
    fn common_escape_setup_does_not_block_safe_text_convergence() {
        let mut baseline = state();
        baseline
            .apply(&bytes_batch(b"\x1b[31mcommon"))
            .expect("common styling applies");
        let mut left = baseline.clone();
        left.apply(&bytes_batch(b" left")).expect("left applies");
        let mut right = baseline;
        right.apply(&bytes_batch(b" right")).expect("right applies");

        let diff = right
            .diff_from(&left)
            .expect("only the safe divergent tails require reconstruction");
        left.apply(diff.instructions())
            .expect("screen diff must apply to delivered state");
        assert_eq!(left.screen().contents(), right.screen().contents());
    }

    #[test]
    fn diverged_incomplete_parser_input_remains_rejected() {
        let mut left = state();
        left.apply(&bytes_batch(b"left\x1b["))
            .expect("incomplete CSI applies");
        let mut right = state();
        right
            .apply(&bytes_batch(b"right\x1b["))
            .expect("incomplete CSI applies");
        assert!(matches!(
            right.diff_from(&left),
            Err(StateError::DivergedHistory)
        ));
    }

    #[test]
    fn diverged_completed_escape_state_converges_through_screen_diff() {
        let mut left = state();
        left.apply(&bytes_batch(b"\x1b[31mleft"))
            .expect("styled text applies");
        let mut right = state();
        right
            .apply(&bytes_batch(b"\x1b[32mright"))
            .expect("styled text applies");
        let diff = right
            .diff_from(&left)
            .expect("completed terminal states must converge");
        left.apply(diff.instructions())
            .expect("screen diff must apply");
        assert_eq!(
            left.screen().state_formatted(),
            right.screen().state_formatted()
        );
    }

    #[test]
    fn clones_share_history_and_append_independent_tails() {
        let mut original = state();
        original
            .apply(&bytes_batch(b"shared"))
            .expect("shared bytes apply");
        let mut cloned = original.clone();
        assert!(std::sync::Arc::ptr_eq(
            original.history.tail.as_ref().expect("history has a tail"),
            cloned.history.tail.as_ref().expect("clone has a tail")
        ));
        cloned
            .apply(&bytes_batch(b" branch"))
            .expect("branch bytes apply");
        assert_eq!(original.retained_bytes(), 6);
        assert_eq!(original.operation_count(), 1);
        assert_eq!(cloned.retained_bytes(), 13);
        assert_eq!(cloned.operation_count(), 2);
        assert_eq!(original.screen().contents(), "shared");
        assert_eq!(cloned.screen().contents(), "shared branch");
    }

    #[test]
    fn history_limit_rejection_is_transactional() {
        let mut state = state();
        let oversized = vec![b'x'; MAX_RETAINED_HOST_BYTES + 1];
        assert_eq!(
            state.apply(&bytes_batch(&oversized)),
            Err(StateError::HistoryLimitExceeded)
        );
        assert_eq!(state.retained_bytes(), 0);
        assert_eq!(state.operation_count(), 0);
        assert!(state.screen().contents().is_empty());
    }

    #[test]
    fn long_running_ground_state_output_compacts_history() {
        let mut state = state();
        let chunk = vec![b'x'; 1024 * 1024];
        for _ in 0..17 {
            state
                .apply(&bytes_batch(&chunk))
                .expect("completed output must compact instead of ending the session");
        }

        assert!(state.retained_bytes() <= MAX_RETAINED_HOST_BYTES);
        let cloned = state.clone();
        assert_eq!(
            cloned.screen().state_formatted(),
            state.screen().state_formatted()
        );
    }

    fn state() -> RemoteTerminalState {
        RemoteTerminalState::new(80, 24).expect("test viewport is valid")
    }

    fn bytes_batch(bytes: &[u8]) -> InstructionBatch {
        InstructionBatch {
            instructions: vec![Instruction {
                bytes: Some(ByteRun {
                    value: bytes.to_vec(),
                }),
                viewport: None,
                marker: None,
                session_control: None,
            }],
        }
    }

    fn host_bytes(batch: &InstructionBatch) -> Vec<u8> {
        batch
            .instructions
            .iter()
            .filter_map(|instruction| instruction.bytes.as_ref())
            .flat_map(|bytes| bytes.value.iter().copied())
            .collect()
    }
}
