// SPDX-License-Identifier: Apache-2.0

//! Conservative terminal-local prediction overlays.

use std::collections::VecDeque;
use std::fmt;

const UNDERLINE_RENDITION: &[u8] = b"\x1b[4m";
const NO_UNDERLINE_RENDITION: &[u8] = b"\x1b[24m";
const INSERT_CHARACTER: &[u8] = b"\x1b[@";
const DELETE_CHARACTER: &[u8] = b"\x1b[P";
const CURSOR_LEFT: &[u8] = b"\x1b[D";
const CURSOR_RIGHT: &[u8] = b"\x1b[C";
const ADAPTIVE_DISPLAY_MILLISECONDS: u16 = 20;
const STALE_PREDICTION_MILLISECONDS: u64 = 500;
const MAXIMUM_PENDING_PREDICTIONS: usize = 1_024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PredictionDisplay {
    #[default]
    Adaptive,
    Always,
    Never,
}

fn single_printable_action(bytes: &[u8]) -> Option<PredictionAction> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut characters = text.chars();
    let character = characters.next()?;
    if characters.next().is_some() || character.is_control() {
        return None;
    }
    if character.is_ascii() {
        Some(PredictionAction::PrintableAscii(character as u8))
    } else {
        Some(PredictionAction::PrintableUtf8(character))
    }
}

/// Identifies input provenance so uncertain multi-byte actions stay
/// ineligible even when their bytes contain printable characters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputKind {
    Key,
    Paste,
    Mouse,
    Focus,
}

/// Describes the small set of single-cell edits that can be predicted without
/// interpreting arbitrary terminal input bytes.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum PredictionAction {
    PrintableAscii(u8),
    PrintableUtf8(char),
    Backspace,
    Left,
    Right,
    Barrier,
}

impl fmt::Debug for PredictionAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PrintableAscii(_) => formatter.write_str("PrintableAscii([REDACTED])"),
            Self::PrintableUtf8(_) => formatter.write_str("PrintableUtf8([REDACTED])"),
            Self::Backspace => formatter.write_str("Backspace"),
            Self::Left => formatter.write_str("Left"),
            Self::Right => formatter.write_str("Right"),
            Self::Barrier => formatter.write_str("Barrier"),
        }
    }
}

/// Content-free counters suitable for diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PredictionMetrics {
    pub offered: u64,
    pub displayed: u64,
    pub latest_acknowledgement: Option<u64>,
    pub reconciled: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PredictionEntry {
    frame_id: u64,
    _action: PredictionAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PredictedCell {
    frame_id: u64,
    acknowledged: bool,
}

/// Authoritative terminal coordinates and rendition captured before a local
/// prediction is displayed. Coordinates are zero based, matching `vt100`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PredictionContext {
    pub row: u16,
    pub column: u16,
    pub columns: u16,
    pub attributes: Vec<u8>,
    pub cursor_state: Vec<u8>,
}

/// Confidence in the currently displayed single-line prediction span.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineConfidence {
    Tentative,
    Acknowledged,
}

/// Confidence associated with one authoritative zero-based terminal row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PredictionLineStatus {
    pub row: u16,
    pub confidence: LineConfidence,
}

/// The smallest safe operation for removing a prediction overlay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PredictionReconciliation {
    None,
    Local(Vec<u8>),
    Redraw,
}

/// Tracks bounded tentative single-column edits. Authoritative terminal or
/// protocol state is never stored here.
pub struct PredictionOverlay {
    display: PredictionDisplay,
    overwrite: bool,
    round_trip_milliseconds: Option<u16>,
    pending: VecDeque<PredictionEntry>,
    predicted_ascii: Vec<PredictedCell>,
    cursor_offset: usize,
    highest_frame_id: Option<u64>,
    displayed_cells: usize,
    rollback_cells: usize,
    overlay_active: bool,
    reconciliation_required: bool,
    oldest_prediction_milliseconds: Option<u64>,
    line_context: Option<PredictionContext>,
    metrics: PredictionMetrics,
}

impl Default for PredictionOverlay {
    fn default() -> Self {
        Self::new(PredictionDisplay::Adaptive, false)
    }
}

impl PredictionOverlay {
    #[must_use]
    pub const fn new(display: PredictionDisplay, overwrite: bool) -> Self {
        Self {
            display,
            overwrite,
            round_trip_milliseconds: None,
            pending: VecDeque::new(),
            predicted_ascii: Vec::new(),
            cursor_offset: 0,
            highest_frame_id: None,
            displayed_cells: 0,
            rollback_cells: 0,
            overlay_active: false,
            reconciliation_required: false,
            oldest_prediction_milliseconds: None,
            line_context: None,
            metrics: PredictionMetrics {
                offered: 0,
                displayed: 0,
                latest_acknowledgement: None,
                reconciled: 0,
            },
        }
    }

    pub fn set_round_trip_milliseconds(&mut self, milliseconds: Option<u16>) {
        self.round_trip_milliseconds = milliseconds;
    }

    /// Returns terminal bytes for an eligible tentative key, or `None` when
    /// the input cannot be predicted conservatively.
    #[must_use]
    pub fn offer(&mut self, kind: InputKind, bytes: &[u8]) -> Option<Vec<u8>> {
        let frame_id = match self.highest_frame_id {
            Some(highest) => highest.checked_add(1)?,
            None => 0,
        };
        let action = if kind == InputKind::Key {
            single_printable_action(bytes).unwrap_or(PredictionAction::Barrier)
        } else {
            PredictionAction::Barrier
        };
        self.offer_for_frame(frame_id, action)
    }

    /// Returns terminal bytes for a conservative edit associated with a
    /// client SSP frame. Frames must be offered in nondecreasing order.
    #[must_use]
    pub fn offer_for_frame(&mut self, frame_id: u64, action: PredictionAction) -> Option<Vec<u8>> {
        self.offer_for_frame_with_context(frame_id, action, None)
    }

    /// Offers a prediction anchored to the authoritative terminal cursor.
    #[must_use]
    pub fn offer_for_frame_with_context(
        &mut self,
        frame_id: u64,
        action: PredictionAction,
        context: Option<&PredictionContext>,
    ) -> Option<Vec<u8>> {
        self.offer_for_frame_with_context_inner(frame_id, action, context, None)
    }

    /// Offers a prediction with the host's monotonic millisecond timestamp.
    ///
    /// Call `expire_stale` with values from the same clock to remove a span
    /// that has remained speculative for too long.
    #[must_use]
    pub fn offer_for_frame_with_context_at(
        &mut self,
        frame_id: u64,
        action: PredictionAction,
        context: Option<&PredictionContext>,
        offered_at_milliseconds: u64,
    ) -> Option<Vec<u8>> {
        self.offer_for_frame_with_context_inner(
            frame_id,
            action,
            context,
            Some(offered_at_milliseconds),
        )
    }

    fn offer_for_frame_with_context_inner(
        &mut self,
        frame_id: u64,
        action: PredictionAction,
        context: Option<&PredictionContext>,
        offered_at_milliseconds: Option<u64>,
    ) -> Option<Vec<u8>> {
        self.metrics.offered = self.metrics.offered.saturating_add(1);
        if self
            .highest_frame_id
            .is_some_and(|highest| frame_id < highest)
        {
            return None;
        }
        self.highest_frame_id = Some(frame_id);

        if action == PredictionAction::Barrier {
            self.invalidate_span();
            return None;
        }

        let display_enabled = match self.display {
            PredictionDisplay::Adaptive => self
                .round_trip_milliseconds
                .is_some_and(|milliseconds| milliseconds >= ADAPTIVE_DISPLAY_MILLISECONDS),
            PredictionDisplay::Always => true,
            PredictionDisplay::Never => false,
        };
        if !display_enabled {
            return None;
        }

        if self.reconciliation_required {
            return None;
        }
        if self.pending.len() >= MAXIMUM_PENDING_PREDICTIONS
            || self.predicted_ascii.len() >= MAXIMUM_PENDING_PREDICTIONS
        {
            self.invalidate_span();
            return None;
        }
        let output = match action {
            PredictionAction::PrintableAscii(byte) if byte.is_ascii_graphic() || byte == b' ' => {
                self.predict_printable(frame_id, &[byte], context)
            }
            PredictionAction::PrintableUtf8(character) if !character.is_control() => {
                let mut encoded = [0; 4];
                self.predict_printable(
                    frame_id,
                    character.encode_utf8(&mut encoded).as_bytes(),
                    context,
                )
            }
            PredictionAction::Backspace => self.predict_backspace(),
            PredictionAction::Left => self.predict_left(),
            PredictionAction::Right => self.predict_right(),
            PredictionAction::PrintableAscii(_) | PredictionAction::PrintableUtf8(_) => {
                self.invalidate_span();
                None
            }
            PredictionAction::Barrier => None,
        };
        if output.is_some() {
            self.pending.push_back(PredictionEntry {
                frame_id,
                _action: action,
            });
            if let Some(offered_at_milliseconds) = offered_at_milliseconds {
                self.oldest_prediction_milliseconds
                    .get_or_insert(offered_at_milliseconds);
            }
            self.metrics.displayed = self.metrics.displayed.saturating_add(1);
        }
        output
    }

    /// Invalidates a speculative span older than mosh-go's prediction timeout.
    ///
    /// Returns `true` when the caller must apply `take_reconciliation` before
    /// drawing any further local overlay or authoritative terminal output.
    pub fn expire_stale(&mut self, now_milliseconds: u64) -> bool {
        let Some(oldest_prediction_milliseconds) = self.oldest_prediction_milliseconds else {
            return false;
        };
        if now_milliseconds.saturating_sub(oldest_prediction_milliseconds)
            < STALE_PREDICTION_MILLISECONDS
        {
            return false;
        }
        self.invalidate_span();
        self.reconciliation_required
    }

    /// Removes every tentative cell and restores the saved cursor before the
    /// caller renders authoritative output or processes a resize.
    #[must_use]
    pub fn reconcile(&mut self) -> bool {
        !matches!(self.take_reconciliation(), PredictionReconciliation::None)
    }

    /// Removes prediction bookkeeping and returns the smallest safe terminal
    /// restoration. Insert-mode spans can be deleted locally; overwrite-mode
    /// spans require an authoritative redraw because overwritten cells are not
    /// retained by this crate.
    #[must_use]
    pub fn take_reconciliation(&mut self) -> PredictionReconciliation {
        if !self.overlay_active && !self.reconciliation_required {
            return PredictionReconciliation::None;
        }
        let local_reconciliation = if self.overwrite {
            None
        } else {
            self.local_reconciliation()
        };
        let displayed_cells = std::mem::take(&mut self.displayed_cells);
        self.metrics.reconciled = self
            .metrics
            .reconciled
            .saturating_add(u64::try_from(displayed_cells).unwrap_or(u64::MAX));
        self.pending.clear();
        self.predicted_ascii.clear();
        self.cursor_offset = 0;
        self.overlay_active = false;
        self.reconciliation_required = false;
        self.rollback_cells = 0;
        self.oldest_prediction_milliseconds = None;
        self.line_context = None;
        local_reconciliation.map_or(
            PredictionReconciliation::Redraw,
            PredictionReconciliation::Local,
        )
    }

    /// Records the newest server `EchoAck` without treating it as terminal
    /// output. Rendering remains tentative until authoritative `HostBytes` arrive.
    pub fn acknowledge(&mut self, acknowledgement: u64) {
        let Some(highest_frame_id) = self.highest_frame_id else {
            return;
        };
        if acknowledgement > highest_frame_id
            || self
                .metrics
                .latest_acknowledgement
                .is_some_and(|current| acknowledgement <= current)
        {
            return;
        }
        self.metrics.latest_acknowledgement = Some(acknowledgement);
        while self
            .pending
            .front()
            .is_some_and(|entry| entry.frame_id <= acknowledgement)
        {
            self.pending.pop_front();
        }
        for cell in &mut self.predicted_ascii {
            if cell.frame_id <= acknowledgement {
                cell.acknowledged = true;
            }
        }
    }

    #[must_use]
    pub const fn metrics(&self) -> PredictionMetrics {
        self.metrics
    }

    /// Returns confidence for the active line. A line is acknowledged only
    /// when every remaining predicted cell has an explicit cumulative `EchoAck`.
    #[must_use]
    pub fn line_confidence(&self) -> Option<LineConfidence> {
        if self.predicted_ascii.is_empty() {
            return None;
        }
        Some(
            if self.predicted_ascii.iter().all(|cell| cell.acknowledged) {
                LineConfidence::Acknowledged
            } else {
                LineConfidence::Tentative
            },
        )
    }

    /// Returns the authoritative row and confidence of the active span.
    #[must_use]
    pub fn line_status(&self) -> Option<PredictionLineStatus> {
        Some(PredictionLineStatus {
            row: self.line_context.as_ref()?.row,
            confidence: self.line_confidence()?,
        })
    }

    fn predict_printable(
        &mut self,
        frame_id: u64,
        bytes: &[u8],
        context: Option<&PredictionContext>,
    ) -> Option<Vec<u8>> {
        if self.cursor_offset != self.predicted_ascii.len() {
            return None;
        }
        if let Some(context) = context {
            let anchor = self.line_context.get_or_insert_with(|| context.clone());
            if anchor.row != context.row
                || anchor.column != context.column
                || anchor.columns != context.columns
                || usize::from(anchor.column).saturating_add(self.cursor_offset)
                    >= usize::from(anchor.columns.saturating_sub(1))
            {
                self.invalidate_span();
                return None;
            }
        }
        self.predicted_ascii.push(PredictedCell {
            frame_id,
            acknowledged: false,
        });
        self.cursor_offset = self.cursor_offset.saturating_add(1);
        self.displayed_cells = self.displayed_cells.saturating_add(1);
        self.rollback_cells = self.rollback_cells.saturating_add(1);
        self.overlay_active = true;

        let mut output = Vec::with_capacity(17 + context.map_or(0, |value| value.attributes.len()));
        if !self.overwrite {
            output.extend_from_slice(INSERT_CHARACTER);
        }
        output.extend_from_slice(UNDERLINE_RENDITION);
        output.extend_from_slice(bytes);
        if let Some(context) = context {
            output.extend_from_slice(&context.attributes);
        } else {
            output.extend_from_slice(NO_UNDERLINE_RENDITION);
        }
        Some(output)
    }

    fn predict_backspace(&mut self) -> Option<Vec<u8>> {
        if self.cursor_offset == 0 || self.cursor_offset != self.predicted_ascii.len() {
            return None;
        }
        if self.predicted_ascii.last().is_some_and(|cell| {
            self.metrics
                .latest_acknowledgement
                .is_some_and(|acknowledgement| cell.frame_id <= acknowledgement)
        }) {
            return None;
        }
        if self.overwrite {
            self.invalidate_span();
            return None;
        }
        self.predicted_ascii.pop();
        self.cursor_offset -= 1;
        self.displayed_cells = self.displayed_cells.saturating_sub(1);
        self.rollback_cells = self.rollback_cells.saturating_sub(1);
        self.overlay_active = true;
        let mut output = Vec::with_capacity(CURSOR_LEFT.len() + DELETE_CHARACTER.len());
        output.extend_from_slice(CURSOR_LEFT);
        output.extend_from_slice(DELETE_CHARACTER);
        Some(output)
    }

    fn predict_left(&mut self) -> Option<Vec<u8>> {
        if self.cursor_offset == 0 {
            return None;
        }
        self.cursor_offset -= 1;
        self.overlay_active = true;
        Some(CURSOR_LEFT.to_vec())
    }

    fn predict_right(&mut self) -> Option<Vec<u8>> {
        if self.cursor_offset >= self.predicted_ascii.len() {
            return None;
        }
        self.cursor_offset += 1;
        self.overlay_active = true;
        Some(CURSOR_RIGHT.to_vec())
    }

    fn invalidate_span(&mut self) {
        self.pending.clear();
        self.predicted_ascii.clear();
        self.cursor_offset = 0;
        self.oldest_prediction_milliseconds = None;
        self.reconciliation_required |= self.overlay_active;
    }

    fn local_reconciliation(&self) -> Option<Vec<u8>> {
        let context = self.line_context.as_ref()?;
        let mut output = Vec::new();
        if self.rollback_cells != 0 {
            output.extend_from_slice(
                format!(
                    "\x1b[{};{}H",
                    context.row.saturating_add(1),
                    context.column.saturating_add(1)
                )
                .as_bytes(),
            );
            output.extend_from_slice(format!("\x1b[{}P", self.rollback_cells).as_bytes());
        }
        output.extend_from_slice(&context.cursor_state);
        Some(output)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        InputKind, LineConfidence, MAXIMUM_PENDING_PREDICTIONS, PredictionAction,
        PredictionContext, PredictionLineStatus, PredictionMetrics, PredictionOverlay,
        PredictionReconciliation,
    };

    #[test]
    fn prediction_action_debug_redacts_printable_input() {
        let output = format!("{:?}", PredictionAction::PrintableAscii(b'X'));
        assert!(!output.contains('X'));
        assert!(output.contains("REDACTED"));
    }

    fn context(row: u16, column: u16, columns: u16) -> PredictionContext {
        PredictionContext {
            row,
            column,
            columns,
            attributes: b"\x1b[0;1m".to_vec(),
            cursor_state: b"\x1b[3;5H\x1b[0;1m".to_vec(),
        }
    }

    #[test]
    fn predicts_single_printable_utf8_keys() {
        let mut overlay = PredictionOverlay::new(super::PredictionDisplay::Always, true);
        assert_eq!(
            overlay.offer(InputKind::Key, b"a"),
            Some(b"\x1b[4ma\x1b[24m".to_vec())
        );
        assert_eq!(
            overlay.offer(InputKind::Key, "é".as_bytes()),
            Some(b"\x1b[4m\xc3\xa9\x1b[24m".to_vec())
        );
        assert_eq!(
            overlay.offer(InputKind::Key, "界".as_bytes()),
            Some("\x1b[4m界\x1b[24m".as_bytes().to_vec())
        );
        assert!(overlay.offer(InputKind::Key, b"\r").is_none());
        assert!(overlay.offer(InputKind::Paste, b"b").is_none());
        assert!(overlay.offer(InputKind::Mouse, b"c").is_none());
        assert!(overlay.offer(InputKind::Focus, b"d").is_none());
    }

    #[test]
    fn reconciliation_clears_overlay_before_authoritative_output() {
        let mut overlay = PredictionOverlay::new(super::PredictionDisplay::Always, true);
        let _ = overlay.offer(InputKind::Key, b"a");
        assert_eq!(
            overlay.offer(InputKind::Key, b"b"),
            Some(b"\x1b[4mb\x1b[24m".to_vec())
        );
        assert!(overlay.reconcile());
        assert!(!overlay.reconcile());
        assert_eq!(
            overlay.metrics(),
            PredictionMetrics {
                offered: 2,
                displayed: 2,
                latest_acknowledgement: None,
                reconciled: 2,
            }
        );
    }

    #[test]
    fn stale_predictions_request_reconciliation_after_mosh_go_timeout() {
        let mut overlay = PredictionOverlay::new(super::PredictionDisplay::Always, false);
        let line = context(2, 4, 80);
        assert!(
            overlay
                .offer_for_frame_with_context_at(
                    1,
                    PredictionAction::PrintableAscii(b'a'),
                    Some(&line),
                    900,
                )
                .is_some()
        );
        assert!(!overlay.expire_stale(1_399));

        // A timed-out speculative span must be removed before further drawing.
        assert!(overlay.expire_stale(1_400));
        assert!(matches!(
            overlay.take_reconciliation(),
            PredictionReconciliation::Local(_)
        ));
        assert!(!overlay.expire_stale(1_900));
    }

    #[test]
    fn acknowledgements_clear_only_known_frames_and_ignore_invalid_values() {
        let mut overlay = PredictionOverlay::new(super::PredictionDisplay::Always, false);
        let _ = overlay.offer_for_frame(4, PredictionAction::PrintableAscii(b'a'));
        let _ = overlay.offer_for_frame(7, PredictionAction::PrintableAscii(b'b'));

        overlay.acknowledge(8);
        assert_eq!(overlay.metrics().latest_acknowledgement, None);
        assert_eq!(overlay.pending.len(), 2);

        overlay.acknowledge(4);
        assert_eq!(overlay.metrics().latest_acknowledgement, Some(4));
        assert_eq!(overlay.pending.len(), 1);

        overlay.acknowledge(4);
        overlay.acknowledge(3);
        assert_eq!(overlay.metrics().latest_acknowledgement, Some(4));
        assert_eq!(overlay.pending.len(), 1);

        overlay.acknowledge(7);
        assert_eq!(overlay.metrics().latest_acknowledgement, Some(7));
        assert!(overlay.pending.is_empty());
    }

    #[test]
    fn echo_ack_promotes_only_cells_in_the_acknowledged_frame_range() {
        let mut overlay = PredictionOverlay::new(super::PredictionDisplay::Always, false);
        let line = context(2, 4, 80);
        let _ = overlay.offer_for_frame_with_context(
            4,
            PredictionAction::PrintableAscii(b'a'),
            Some(&line),
        );
        let _ = overlay.offer_for_frame_with_context(
            7,
            PredictionAction::PrintableAscii(b'b'),
            Some(&line),
        );

        overlay.acknowledge(4);
        assert_eq!(overlay.line_confidence(), Some(LineConfidence::Tentative));
        assert_eq!(
            overlay.line_status(),
            Some(PredictionLineStatus {
                row: 2,
                confidence: LineConfidence::Tentative,
            })
        );
        overlay.acknowledge(7);
        assert_eq!(
            overlay.line_confidence(),
            Some(LineConfidence::Acknowledged)
        );
    }

    #[test]
    fn insertion_overlay_is_removed_with_a_local_line_edit() {
        let mut overlay = PredictionOverlay::new(super::PredictionDisplay::Always, false);
        let line = context(2, 4, 80);
        assert_eq!(
            overlay.offer_for_frame_with_context(
                1,
                PredictionAction::PrintableAscii(b'a'),
                Some(&line),
            ),
            Some(b"\x1b[@\x1b[4ma\x1b[0;1m".to_vec())
        );

        assert_eq!(
            overlay.take_reconciliation(),
            PredictionReconciliation::Local(b"\x1b[3;5H\x1b[1P\x1b[3;5H\x1b[0;1m".to_vec())
        );
    }

    #[test]
    fn barrier_preserves_the_local_rollback_extent() {
        let mut overlay = PredictionOverlay::new(super::PredictionDisplay::Always, false);
        let line = context(2, 4, 80);
        let _ = overlay.offer_for_frame_with_context(
            1,
            PredictionAction::PrintableAscii(b'a'),
            Some(&line),
        );
        let _ = overlay.offer_for_frame_with_context(2, PredictionAction::Barrier, Some(&line));

        assert_eq!(
            overlay.take_reconciliation(),
            PredictionReconciliation::Local(b"\x1b[3;5H\x1b[1P\x1b[3;5H\x1b[0;1m".to_vec())
        );
    }

    #[test]
    fn prediction_does_not_cross_the_authoritative_line_boundary() {
        let mut overlay = PredictionOverlay::new(super::PredictionDisplay::Always, false);
        let last_column = context(0, 79, 80);

        assert!(
            overlay
                .offer_for_frame_with_context(
                    1,
                    PredictionAction::PrintableAscii(b'a'),
                    Some(&last_column),
                )
                .is_none()
        );
    }

    #[test]
    fn prediction_display_and_overwrite_settings_are_applied() {
        let mut never = PredictionOverlay::new(super::PredictionDisplay::Never, false);
        assert!(never.offer(InputKind::Key, b"a").is_none());

        let mut adaptive = PredictionOverlay::default();
        adaptive.set_round_trip_milliseconds(Some(19));
        assert!(adaptive.offer(InputKind::Key, b"a").is_none());
        adaptive.set_round_trip_milliseconds(Some(20));
        assert_eq!(
            adaptive.offer(InputKind::Key, b"a"),
            Some(b"\x1b[@\x1b[4ma\x1b[24m".to_vec())
        );
    }

    #[test]
    fn backspace_cancels_only_the_last_unacknowledged_printable() {
        let mut overlay = PredictionOverlay::new(super::PredictionDisplay::Always, false);
        let _ = overlay.offer_for_frame(1, PredictionAction::PrintableAscii(b'a'));
        let _ = overlay.offer_for_frame(2, PredictionAction::PrintableAscii(b'b'));
        assert_eq!(
            overlay.offer_for_frame(3, PredictionAction::Backspace),
            Some(b"\x1b[D\x1b[P".to_vec())
        );
        assert_eq!(overlay.predicted_ascii.len(), 1);

        overlay.acknowledge(3);
        assert!(
            overlay
                .offer_for_frame(4, PredictionAction::Backspace)
                .is_none()
        );
        assert_eq!(overlay.predicted_ascii.len(), 1);
    }

    #[test]
    fn cursor_motion_stays_inside_the_predicted_span() {
        let mut overlay = PredictionOverlay::new(super::PredictionDisplay::Always, false);
        assert!(overlay.offer_for_frame(1, PredictionAction::Left).is_none());
        let _ = overlay.offer_for_frame(2, PredictionAction::PrintableAscii(b'a'));
        let _ = overlay.offer_for_frame(3, PredictionAction::PrintableAscii(b'b'));
        assert_eq!(
            overlay.offer_for_frame(4, PredictionAction::Left),
            Some(b"\x1b[D".to_vec())
        );
        assert!(
            overlay
                .offer_for_frame(5, PredictionAction::PrintableAscii(b'c'))
                .is_none()
        );
        assert_eq!(
            overlay.offer_for_frame(6, PredictionAction::Right),
            Some(b"\x1b[C".to_vec())
        );
        assert!(
            overlay
                .offer_for_frame(7, PredictionAction::Right)
                .is_none()
        );
    }

    #[test]
    fn barrier_invalidates_the_span_and_requires_reconciliation() {
        let mut overlay = PredictionOverlay::new(super::PredictionDisplay::Always, false);
        let _ = overlay.offer_for_frame(1, PredictionAction::PrintableAscii(b'a'));
        assert!(
            overlay
                .offer_for_frame(2, PredictionAction::Barrier)
                .is_none()
        );
        assert!(overlay.pending.is_empty());
        assert!(overlay.predicted_ascii.is_empty());
        assert!(overlay.reconcile());
        assert!(!overlay.reconcile());
    }

    #[test]
    fn pending_prediction_memory_is_bounded() {
        let mut overlay = PredictionOverlay::new(super::PredictionDisplay::Always, false);
        for frame_id in 0..MAXIMUM_PENDING_PREDICTIONS {
            assert!(
                overlay
                    .offer_for_frame(
                        u64::try_from(frame_id).expect("test frame must fit u64"),
                        PredictionAction::PrintableAscii(b'a'),
                    )
                    .is_some()
            );
        }
        assert_eq!(overlay.pending.len(), MAXIMUM_PENDING_PREDICTIONS);
        assert!(
            overlay
                .offer_for_frame(
                    u64::try_from(MAXIMUM_PENDING_PREDICTIONS)
                        .expect("prediction bound must fit u64"),
                    PredictionAction::PrintableAscii(b'b'),
                )
                .is_none()
        );
        assert!(overlay.pending.is_empty());
        assert!(overlay.reconcile());
    }

    #[test]
    fn overwrite_backspace_requires_authoritative_reconciliation() {
        let mut overlay = PredictionOverlay::new(super::PredictionDisplay::Always, true);
        let _ = overlay.offer_for_frame(1, PredictionAction::PrintableAscii(b'a'));
        assert!(
            overlay
                .offer_for_frame(2, PredictionAction::Backspace)
                .is_none()
        );
        assert!(overlay.reconcile());
    }
}
