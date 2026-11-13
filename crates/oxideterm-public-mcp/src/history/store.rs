use super::{HistoryArgs, HistoryBuffer, HistorySelection};
use crate::{ClientRef, TerminalRef, ToolEnvelope, ToolOutcome};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;
use zeroize::Zeroizing;

pub const HISTORY_TTL: Duration = Duration::from_secs(300);
pub const HISTORY_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
const TOTAL_BYTES: usize = 128 * 1024 * 1024;
const RECORD_BYTES: usize = 2048;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryStamp {
    pub identity: u64,
    pub content: u64,
    pub switches: u64,
    pub cleared: u64,
    pub evicted: usize,
}
/// Owned text is erased with its snapshot; Debug never exposes terminal output.
pub struct HistoryRow {
    pub text: Zeroizing<String>,
    pub soft_wrapped: bool,
}
pub struct HistoryCapture {
    pub stamp: HistoryStamp,
    pub captured: Instant,
    pub wall_time: SystemTime,
    pub screen_rows: usize,
    pub retained_rows: usize,
    pub active_buffer: HistoryBuffer,
    pub rows: Option<Vec<HistoryRow>>,
}
impl HistoryCapture {
    fn bytes(&self) -> usize {
        RECORD_BYTES
            + self.rows.as_ref().map_or(0, |rows| {
                rows.capacity() * std::mem::size_of::<HistoryRow>()
                    + rows.iter().map(|r| r.text.capacity()).sum::<usize>()
            })
    }
}
#[derive(Clone)]
pub struct HistoryTarget {
    pub terminal: TerminalRef,
    pub session: String,
}
#[derive(Clone)]
struct Revision {
    client: ClientRef,
    token: String,
    target: HistoryTarget,
    buffer: HistoryBuffer,
    selection: HistorySelection,
    stamp: HistoryStamp,
    captured: Instant,
    wall: SystemTime,
}
pub struct HistoryReservation {
    client: ClientRef,
    slot: u8,
    id: String,
    target: HistoryTarget,
    buffer: HistoryBuffer,
    selection: HistorySelection,
    previous: Option<Revision>,
    unknown: bool,
}
impl HistoryReservation {
    pub fn id_for_cleanup(&self) -> String {
        self.id.clone()
    }
    pub fn revision_valid_at(&self, now: Instant) -> bool {
        self.previous
            .as_ref()
            .is_some_and(|r| now.saturating_duration_since(r.captured) < HISTORY_TTL)
    }
    pub fn unchanged_stamp(&self) -> Option<&HistoryStamp> {
        self.previous
            .as_ref()
            .filter(|r| r.target.session == self.target.session && r.selection == self.selection)
            .map(|r| &r.stamp)
    }
}
struct Snapshot {
    id: String,
    target: HistoryTarget,
    capture: HistoryCapture,
    metadata: Value,
    cursor_key: super::cursor::CursorKey,
    bytes: usize,
}
struct ReservationState {
    client: ClientRef,
    terminal: TerminalRef,
    valid: bool,
}
impl Snapshot {
    fn position(&self, cursor: &str) -> Option<(usize, usize)> {
        if cursor == self.id {
            return Some((0, 0));
        }
        let (row, offset) = self.cursor_key.decode(&self.id, cursor)?;
        let text = &self.capture.rows.as_ref()?.get(row)?.text;
        (offset <= text.len() && text.is_char_boundary(offset)).then_some((row, offset))
    }
}
#[derive(Default)]
pub struct HistoryStore {
    snapshots: HashMap<(ClientRef, u8), Snapshot>,
    revisions: VecDeque<Revision>,
    reservations: HashMap<String, ReservationState>,
    gates: HashMap<(ClientRef, u8), Arc<AsyncMutex<()>>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryError {
    InvalidRequest,
    InvalidRevision,
    TerminalUnavailable,
    BufferUnavailable,
    Unsupported,
    SnapshotUnavailable,
    SnapshotTooLarge,
    ResourceLimit,
}
impl HistoryError {
    pub fn reason(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::InvalidRevision => "invalid_revision",
            Self::TerminalUnavailable => "terminal_unavailable",
            Self::BufferUnavailable => "buffer_unavailable",
            Self::Unsupported => "unsupported",
            Self::SnapshotUnavailable => "snapshot_unavailable",
            Self::SnapshotTooLarge => "snapshot_too_large",
            Self::ResourceLimit => "resource_limit",
        }
    }
    pub fn envelope(self) -> ToolEnvelope {
        ToolEnvelope { outcome: ToolOutcome::Failed, data: Some(json!({"reason": self.reason()})), error: Some(match self {
        Self::SnapshotUnavailable => "The snapshot is unavailable. Request a new snapshot without after_revision.",
        Self::ResourceLimit => "The history cache is full. Release an owned snapshot or slot before retrying.",
        _ => "The terminal history request could not be completed.",
    }.into()) }
    }
}
fn token() -> String {
    format!("history_{}", Uuid::new_v4().simple())
}
fn timestamp(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
impl HistoryStore {
    pub fn gate(&mut self, client: &ClientRef, slot: u8) -> Arc<AsyncMutex<()>> {
        self.gates
            .entry((client.clone(), slot))
            .or_default()
            .clone()
    }
    pub fn expire(&mut self, now: Instant) {
        self.snapshots
            .retain(|_, s| now.saturating_duration_since(s.capture.captured) < HISTORY_TTL);
        self.revisions
            .retain(|r| now.saturating_duration_since(r.captured) < HISTORY_TTL);
    }
    fn bytes(&self) -> usize {
        self.snapshots.values().map(|s| s.bytes).sum::<usize>()
            + self.revisions.len() * RECORD_BYTES
            + self.reservations.len() * HISTORY_SNAPSHOT_BYTES
    }
    pub fn revoke_client(&mut self, client: &ClientRef) {
        self.snapshots.retain(|(c, _), _| c != client);
        self.revisions.retain(|r| &r.client != client);
        // Revocation forbids publication immediately, but the worker still owns its text.
        // Keep charging its reservation until publication rejects/drops it or cleanup cancels it.
        for state in self
            .reservations
            .values_mut()
            .filter(|r| &r.client == client)
        {
            state.valid = false;
        }
        self.gates.retain(|(c, _), _| c != client);
    }
    pub fn revoke_terminal(&mut self, terminal: &TerminalRef, replaced: bool) {
        self.snapshots.retain(|_, s| &s.target.terminal != terminal);
        for state in self
            .reservations
            .values_mut()
            .filter(|r| &r.terminal == terminal)
        {
            state.valid = false;
        }
        if !replaced {
            self.revisions.retain(|r| &r.target.terminal != terminal);
        }
    }
    pub fn release(&mut self, client: &ClientRef, id: &str) {
        self.snapshots.retain(|(c, _), s| c != client || s.id != id);
    }
    pub fn release_slot(&mut self, client: &ClientRef, slot: u8) {
        self.snapshots.remove(&(client.clone(), slot));
    }
    pub fn cursor_slot(&self, client: &ClientRef, cursor: &str) -> Result<u8, HistoryError> {
        (0..2)
            .find(|slot| {
                self.snapshots
                    .get(&(client.clone(), *slot))
                    .is_some_and(|s| s.position(cursor).is_some())
            })
            .ok_or(HistoryError::SnapshotUnavailable)
    }

    pub fn target(&self, client: &ClientRef, cursor: &str) -> Result<HistoryTarget, HistoryError> {
        let slot = self.cursor_slot(client, cursor)?;
        Ok(self.snapshots[&(client.clone(), slot)].target.clone())
    }
    pub fn reserve(
        &mut self,
        client: &ClientRef,
        slot: u8,
        target: HistoryTarget,
        buffer: HistoryBuffer,
        selection: HistorySelection,
        after: Option<&str>,
        now: Instant,
    ) -> Result<HistoryReservation, HistoryError> {
        self.expire(now);
        if after.is_some_and(|v| !super::valid_token(v)) {
            return Err(HistoryError::InvalidRevision);
        }
        let previous = after
            .and_then(|id| {
                self.revisions
                    .iter()
                    .find(|r| &r.client == client && r.token == id)
            })
            .cloned();
        if previous
            .as_ref()
            .is_some_and(|r| r.target.terminal != target.terminal || r.buffer != buffer)
        {
            return Err(HistoryError::InvalidRevision);
        }
        if self.bytes().saturating_add(HISTORY_SNAPSHOT_BYTES) > TOTAL_BYTES {
            return Err(HistoryError::ResourceLimit);
        }
        let id = token();
        self.reservations.insert(
            id.clone(),
            ReservationState {
                client: client.clone(),
                terminal: target.terminal.clone(),
                valid: true,
            },
        );
        Ok(HistoryReservation {
            client: client.clone(),
            slot,
            id,
            target,
            buffer,
            selection,
            unknown: after.is_some() && previous.is_none(),
            previous,
        })
    }
    pub fn cancel_id(&mut self, id: &str) {
        self.reservations.remove(id);
    }
    pub fn cancel(&mut self, reservation: &HistoryReservation) {
        self.reservations.remove(&reservation.id);
    }
    pub fn publish(
        &mut self,
        reservation: HistoryReservation,
        capture: HistoryCapture,
        args: &HistoryArgs,
        now: Instant,
    ) -> Result<Value, HistoryError> {
        if !self
            .reservations
            .remove(&reservation.id)
            .is_some_and(|r| r.valid)
        {
            return Err(HistoryError::TerminalUnavailable);
        }
        if now.saturating_duration_since(capture.captured) >= HISTORY_TTL {
            return Err(HistoryError::SnapshotUnavailable);
        }
        let HistoryReservation {
            client,
            slot,
            target,
            buffer,
            selection,
            mut previous,
            mut unknown,
            ..
        } = reservation;
        if previous
            .as_ref()
            .is_some_and(|r| capture.captured.saturating_duration_since(r.captured) >= HISTORY_TTL)
        {
            previous = None;
            unknown = true;
        }
        let mut reasons = Vec::new();
        if let Some(old) = &previous {
            if old.target.session != target.session || old.stamp.identity != capture.stamp.identity
            {
                reasons.push("session_replaced");
            } else {
                if old.stamp.cleared != capture.stamp.cleared {
                    reasons.push("buffer_cleared");
                }
                if old.stamp.evicted != capture.stamp.evicted {
                    reasons.push("history_evicted");
                }
            }
        }
        let continuity = if unknown {
            "unknown"
        } else if previous.is_none() {
            "baseline"
        } else if reasons.is_empty() {
            "no_retention_loss"
        } else {
            "gap"
        };
        let unchanged = capture.rows.is_none();
        if unchanged
            && previous
                .as_ref()
                .is_some_and(|r| now.saturating_duration_since(r.captured) >= HISTORY_TTL)
        {
            return Err(HistoryError::SnapshotUnavailable);
        }
        if unchanged
            && !previous.as_ref().is_some_and(|r| {
                r.target.session == target.session
                    && r.stamp == capture.stamp
                    && r.selection == selection
            })
        {
            return Err(HistoryError::InvalidRequest);
        }
        let revision = if unchanged {
            previous.clone().ok_or(HistoryError::InvalidRevision)?
        } else {
            Revision {
                client: client.clone(),
                token: token(),
                target: target.clone(),
                buffer,
                selection,
                stamp: capture.stamp.clone(),
                captured: capture.captured,
                wall: capture.wall_time,
            }
        };
        let total_rows = if matches!(selection, HistorySelection::Screen {}) {
            capture.screen_rows
        } else {
            capture.retained_rows
        };
        let start_row = capture.retained_rows - total_rows;
        let mut metadata = json!({
            "status": if unchanged {"unchanged"} else {"snapshot"},
            "content_revision": revision.token, "revision_expires_at":timestamp(revision.wall+HISTORY_TTL),
            "buffer":buffer, "active_buffer":capture.active_buffer,"screen_rows":capture.screen_rows,
            "coverage":{"selection":selection,"retained_rows":capture.retained_rows,"start_row":start_row,"end_row_exclusive":capture.retained_rows,"has_earlier":start_row>0},
            "continuity":continuity,"gap_reasons":reasons,"continuity_reason":if unknown {Some("revision_unavailable")} else {None},
        });
        if unchanged {
            return Ok(metadata);
        }
        let bytes = capture.bytes();
        if bytes + RECORD_BYTES > HISTORY_SNAPSHOT_BYTES {
            return Err(HistoryError::SnapshotTooLarge);
        }
        self.expire(now);
        while self.revisions.iter().filter(|r| r.client == client).count() >= 256 {
            if let Some(index) = self.revisions.iter().position(|r| r.client == client) {
                self.revisions.remove(index);
            }
        }
        if self.bytes() + bytes + RECORD_BYTES > TOTAL_BYTES {
            return Err(HistoryError::ResourceLimit);
        }
        let id = token();
        metadata["snapshot_ref"] = json!(id);
        metadata["captured_at"] = json!(timestamp(capture.wall_time));
        metadata["expires_at"] = json!(timestamp(capture.wall_time + HISTORY_TTL));
        metadata["total_rows"] = json!(total_rows);
        metadata["total_text_bytes"] = json!(
            capture
                .rows
                .as_ref()
                .unwrap()
                .iter()
                .map(|r| r.text.len())
                .sum::<usize>()
        );
        self.revisions.push_back(revision);
        self.snapshots.insert(
            (client.clone(), slot),
            Snapshot {
                id: id.clone(),
                target,
                capture,
                metadata,
                cursor_key: super::cursor::CursorKey::new(),
                bytes,
            },
        );
        self.page(&client, &id, args, now)
    }
    pub fn page(
        &mut self,
        client: &ClientRef,
        cursor: &str,
        args: &HistoryArgs,
        now: Instant,
    ) -> Result<Value, HistoryError> {
        self.expire(now);
        let slot = self.cursor_slot(client, cursor)?;
        let s = self.snapshots.get_mut(&(client.clone(), slot)).unwrap();
        let (mut row, mut offset) = s
            .position(cursor)
            .ok_or(HistoryError::SnapshotUnavailable)?;
        let rows = s.capture.rows.as_ref().unwrap();
        let mut result = s.metadata.clone();
        let mut fragments = Vec::new();
        let mut bytes = 0;
        // MCP emits both structured content and a JSON text representation. Account for
        // both escape layers and per-fragment metadata before copying terminal text.
        let mut wire = serde_json::to_vec(&result)
            .map_err(|_| HistoryError::InvalidRequest)?
            .len()
            * 3
            + 2048;
        while row < rows.len() && fragments.len() < args.lines() {
            let source = &rows[row];
            let remaining = args
                .bytes()
                .saturating_sub(bytes)
                .min((512 * 1024usize).saturating_sub(wire + 768) / 14);
            let mut end = source.text.len().min(offset + remaining);
            while !source.text.is_char_boundary(end) {
                end -= 1;
            }
            if end == offset && offset < source.text.len() {
                break;
            }
            let complete = end == source.text.len();
            fragments.push(json!({"row_index":row,"text":&source.text[offset..end],"byte_start":offset,"row_complete":complete,"soft_wrapped":source.soft_wrapped}));
            wire += (end - offset) * 14 + 768;
            bytes += end - offset;
            if complete {
                row += 1;
                offset = 0;
            } else {
                offset = end;
                break;
            }
        }
        let next = (row < rows.len()).then(|| s.cursor_key.encode(&s.id, row, offset));
        result["rows"] = Value::Array(fragments);
        result["has_more"] = json!(next.is_some());
        result["next_cursor"] = json!(next);
        Ok(result)
    }
}
