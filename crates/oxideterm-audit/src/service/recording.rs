use super::{AuditClient, HealthState};
mod budget;
mod worker;
use crate::{
    AuditContext, AuditError, AuditStore, RecordingDetails, RecordingFrame, RecordingFrameKind,
};
use async_channel::Sender;
use budget::{RecordingBudget, Reservation};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
pub(super) use worker::{RecordingRequest, RecordingWorker};
use zeroize::Zeroizing;

pub(super) const FLUSH_INTERVAL: Duration = Duration::from_millis(100);
const PRUNE_INTERVAL: Duration = Duration::from_secs(5);
const OUTPUT_BATCH_BYTES: usize = 32 * 1024;
// Include vector growth and derived grid/gap metadata alongside payload bytes.
// Encoding/encryption scratch remains separately bounded by the chunk limit.
const FRAME_RESERVATION_BYTES: usize = 4 * std::mem::size_of::<OwnedFrame>();
const MAX_RECORDING_QUEUE_SLOTS: usize = 384;
// Leave room for the final frame while amortizing durable file and index commits.
const GLOBAL_BUFFER_BYTES: usize = 3 * 1024 * 1024 + 512 * 1024;

pub struct RecordingSink {
    state: Arc<SinkState>,
}

#[derive(Default)]
struct PendingBatch {
    frames: Vec<OwnedFrame>,
    bytes: usize,
    grid: u32,
}

struct SinkState {
    id: u64,
    instance_id: String,
    details: RecordingDetails,
    sender: Sender<RecordingRequest>,
    enabled: Arc<AtomicBool>,
    accepting: Arc<AtomicBool>,
    gate: Mutex<PendingBatch>,
    budget: Arc<RecordingBudget>,
    output_queued: Arc<AtomicUsize>,
    queued: AtomicUsize,
    waiting: AtomicBool,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    generation: Arc<AtomicU64>,
    closed: AtomicBool,
    interrupted: AtomicBool,
    owners: AtomicUsize,
    in_flight: AtomicUsize,
    grid: AtomicU32,
    ever_output: AtomicBool,
    lost_bytes: AtomicU64,
    missed_events: AtomicU64,
    unknown_gap: AtomicBool,
    started_at_ms: i64,
    started_at: Instant,
}

pub(super) enum RecordingMessage {
    Batch {
        sink_id: u64,
        frames: Vec<OwnedFrame>,
        grid: u32,
    },
    Resize {
        sink_id: u64,
        occurred_at_ms: i64,
        columns: u16,
        rows: u16,
        reservation: Reservation,
    },
}

impl RecordingMessage {
    fn sink_id(&self) -> u64 {
        match self {
            Self::Batch { sink_id, .. } | Self::Resize { sink_id, .. } => *sink_id,
        }
    }
}

pub(super) struct RecordingRuntime {
    pub(super) sender: Sender<RecordingRequest>,
    enabled: Arc<AtomicBool>,
    accepting: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    sinks: Arc<Mutex<HashMap<u64, Arc<SinkState>>>>,
    next_id: Arc<AtomicU64>,
    budget: Arc<RecordingBudget>,
    output_queued: Arc<AtomicUsize>,
}

impl RecordingRuntime {
    pub fn wake_waiters(&self) {
        let callbacks = self
            .sinks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|sink| sink.waiting.load(Ordering::Acquire))
            .filter_map(|sink| sink.wake.lock().unwrap_or_else(|e| e.into_inner()).clone())
            .collect::<Vec<_>>();
        for callback in callbacks {
            callback();
        }
    }

    pub fn new() -> (Self, async_channel::Receiver<RecordingRequest>) {
        let (sender, receiver) = async_channel::bounded(MAX_RECORDING_QUEUE_SLOTS + 16);
        (
            Self {
                sender,
                enabled: Arc::new(AtomicBool::new(false)),
                accepting: Arc::new(AtomicBool::new(true)),
                generation: Arc::new(AtomicU64::new(0)),
                sinks: Arc::new(Mutex::new(HashMap::new())),
                next_id: Arc::new(AtomicU64::new(1)),
                budget: Arc::new(RecordingBudget::default()),
                output_queued: Arc::new(AtomicUsize::new(0)),
            },
            receiver,
        )
    }
}

impl AuditClient {
    pub fn recording_sink(&self, context: &AuditContext) -> RecordingSink {
        let id = self.recording.next_id.fetch_add(1, Ordering::Relaxed);
        let state = Arc::new(SinkState {
            id,
            instance_id: context.instance_id().to_owned(),
            details: RecordingDetails {
                session_id: context.session_id.clone().unwrap_or_default(),
                transport_id: context.transport_id.clone(),
                consumer_id: context.consumer_id.clone(),
                operation_id: context.parent_id.clone(),
                endpoint: context.target.clone(),
            },
            sender: self.recording.sender.clone(),
            enabled: self.recording.enabled.clone(),
            accepting: self.recording.accepting.clone(),
            gate: Mutex::new(PendingBatch::default()),
            budget: self.recording.budget.clone(),
            output_queued: self.recording.output_queued.clone(),
            queued: AtomicUsize::new(0),
            waiting: AtomicBool::new(false),
            wake: Mutex::new(None),
            generation: self.recording.generation.clone(),
            closed: AtomicBool::new(false),
            interrupted: AtomicBool::new(false),
            owners: AtomicUsize::new(1),
            in_flight: AtomicUsize::new(0),
            grid: AtomicU32::new(0),
            ever_output: AtomicBool::new(false),
            lost_bytes: AtomicU64::new(0),
            missed_events: AtomicU64::new(0),
            unknown_gap: AtomicBool::new(false),
            started_at_ms: crate::model::now_ms(),
            started_at: Instant::now(),
        });
        self.recording
            .sinks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, state.clone());
        RecordingSink { state }
    }
}

impl Clone for RecordingSink {
    fn clone(&self) -> Self {
        self.state.owners.fetch_add(1, Ordering::Relaxed);
        Self {
            state: self.state.clone(),
        }
    }
}

impl Drop for RecordingSink {
    fn drop(&mut self) {
        if self.state.owners.fetch_sub(1, Ordering::AcqRel) == 1
            && !self.state.closed.load(Ordering::Acquire)
        {
            self.interrupt();
        }
    }
}

impl RecordingSink {
    pub fn is_enabled(&self) -> bool {
        !self.state.closed.load(Ordering::Acquire)
            && !self.state.sender.is_closed()
            && !self.state.details.session_id.is_empty()
            && self.state.enabled.load(Ordering::Acquire)
            && self.state.accepting.load(Ordering::Acquire)
    }

    pub fn generation(&self) -> u64 {
        self.state.generation.load(Ordering::Acquire)
    }

    pub fn set_wake_callback(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        *self.state.wake.lock().unwrap_or_else(|e| e.into_inner()) = Some(callback);
    }

    /// Returns the accepted prefix. A zero/short prefix leaves the suffix with
    /// the caller; retry it after the capacity callback without parsing it again.
    pub fn try_record_output(&self, bytes: &[u8]) -> Result<usize, AuditError> {
        if !self.is_enabled() {
            return Err(AuditError::Closed);
        }
        self.state.waiting.store(true, Ordering::Release);
        self.state.in_flight.fetch_add(1, Ordering::AcqRel);
        let mut processed = 0;
        if let Ok(mut pending) = self.state.gate.try_lock() {
            if self.is_enabled() {
                for part in bytes.chunks(OUTPUT_BATCH_BYTES) {
                    if pending.bytes + part.len() > OUTPUT_BATCH_BYTES
                        && !self.state.send_pending(&mut pending)
                    {
                        break;
                    }
                    let Some(reservation) = self
                        .state
                        .budget
                        .reserve(self.state.id, part.len() + FRAME_RESERVATION_BYTES)
                    else {
                        break;
                    };
                    if pending.frames.is_empty() {
                        pending.grid = self.state.grid.load(Ordering::Acquire);
                    }
                    pending.frames.push(OwnedFrame {
                        occurred_at_ms: self.state.now_ms(),
                        kind: OwnedFrameKind::Output(Zeroizing::new(part.to_vec())),
                        _reservation: Some(reservation),
                    });
                    self.state.ever_output.store(true, Ordering::Release);
                    pending.bytes += part.len();
                    processed += part.len();
                    if pending.bytes == OUTPUT_BATCH_BYTES && !self.state.send_pending(&mut pending)
                    {
                        break;
                    }
                }
            }
        }
        self.state.in_flight.fetch_sub(1, Ordering::AcqRel);
        if processed == bytes.len() {
            self.state.waiting.store(false, Ordering::Release);
        }
        Ok(processed)
    }

    pub fn record_output(&self, bytes: &[u8]) {
        if let Ok(accepted) = self.try_record_output(bytes) {
            if accepted < bytes.len() {
                self.state.note_loss((bytes.len() - accepted) as u64);
                // Best-effort callers discard this suffix; they will not retry
                // the admission turn, so they must not hold up other streams.
                self.state.budget.cancel_waiter(self.state.id);
            }
        }
    }

    /// Preserves resize ordering without dropping it when the recording queue is full.
    pub fn try_resize(&self, columns: u16, rows: u16) -> Result<bool, AuditError> {
        if !self.is_enabled() {
            return Err(AuditError::Closed);
        }
        self.state.waiting.store(true, Ordering::Release);
        let Ok(mut pending) = self.state.gate.try_lock() else {
            return Ok(false);
        };
        if !self.is_enabled() {
            return Err(AuditError::Closed);
        }
        if !self.state.send_pending(&mut pending) {
            return Ok(false);
        }
        let grid = (u32::from(columns) << 16) | u32::from(rows);
        if !self.state.ever_output.load(Ordering::Acquire) {
            self.state.grid.store(grid, Ordering::Release);
            self.state.waiting.store(false, Ordering::Release);
            return Ok(true);
        }
        let Some(reservation) = self
            .state
            .budget
            .reserve(self.state.id, FRAME_RESERVATION_BYTES)
        else {
            return Ok(false);
        };
        if !self.state.reserve_slot() {
            return Ok(false);
        }
        self.state.queued.fetch_add(1, Ordering::AcqRel);
        let result =
            self.state
                .sender
                .try_send(RecordingRequest::Frame(RecordingMessage::Resize {
                    sink_id: self.state.id,
                    occurred_at_ms: self.state.now_ms(),
                    columns,
                    rows,
                    reservation,
                }));
        if result.is_err() {
            self.state.queued.fetch_sub(1, Ordering::AcqRel);
            self.state.output_queued.fetch_sub(1, Ordering::AcqRel);
            return Err(AuditError::Closed);
        }
        self.state.grid.store(grid, Ordering::Release);
        self.state.waiting.store(false, Ordering::Release);
        Ok(true)
    }

    pub fn resize(&self, columns: u16, rows: u16) {
        if !self.is_enabled() {
            self.state.grid.store(
                (u32::from(columns) << 16) | u32::from(rows),
                Ordering::Release,
            );
            return;
        }
        if matches!(self.try_resize(columns, rows), Ok(false)) {
            self.state.budget.cancel_waiter(self.state.id);
            self.state.grid.store(
                (u32::from(columns) << 16) | u32::from(rows),
                Ordering::Release,
            );
            self.state.unknown_gap.store(true, Ordering::Release);
            self.state.missed_events.fetch_add(1, Ordering::AcqRel);
        }
    }

    pub fn close(&self) {
        self.finish(false);
    }
    pub fn interrupt(&self) {
        self.finish(true);
    }

    fn finish(&self, interrupted: bool) {
        let mut pending = self
            .state
            .gate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if !self.state.closed.load(Ordering::Acquire) {
            self.state.send_pending(&mut pending);
            self.state.closed.store(true, Ordering::Release);
        }
        if interrupted {
            self.state.interrupted.store(true, Ordering::Release);
        }
        self.state.budget.cancel_waiter(self.state.id);
    }
}

impl SinkState {
    fn now_ms(&self) -> i64 {
        self.started_at_ms
            .saturating_add(self.started_at.elapsed().as_millis().min(i64::MAX as u128) as i64)
    }

    fn reserve_slot(&self) -> bool {
        self.output_queued
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < MAX_RECORDING_QUEUE_SLOTS).then_some(current + 1)
            })
            .is_ok()
    }

    fn note_loss(&self, bytes: u64) {
        if bytes > 0 {
            self.lost_bytes.fetch_add(bytes, Ordering::AcqRel);
        }
        self.missed_events.fetch_add(1, Ordering::AcqRel);
    }

    fn take_pending(&self, pending: &mut PendingBatch) -> Option<RecordingMessage> {
        if pending.bytes == 0 {
            return None;
        }
        pending.bytes = 0;
        Some(RecordingMessage::Batch {
            sink_id: self.id,
            frames: std::mem::take(&mut pending.frames),
            grid: pending.grid,
        })
    }

    fn send_pending(&self, pending: &mut PendingBatch) -> bool {
        if pending.bytes == 0 {
            return true;
        }
        if !self.reserve_slot() {
            return false;
        }
        let bytes = pending.bytes;
        let message = self
            .take_pending(pending)
            .expect("nonempty recording batch");
        self.queued.fetch_add(1, Ordering::AcqRel);
        if self
            .sender
            .try_send(RecordingRequest::Frame(message))
            .is_err()
        {
            self.queued.fetch_sub(1, Ordering::AcqRel);
            self.output_queued.fetch_sub(1, Ordering::AcqRel);
            self.note_loss(bytes as u64);
            return false;
        }
        true
    }

    fn clear_pending(&self) {
        let mut pending = self.gate.lock().unwrap_or_else(|error| error.into_inner());
        pending.bytes = 0;
        pending.frames = Vec::new();
        self.budget.cancel_waiter(self.id);
    }
}

enum OwnedFrameKind {
    Output(Zeroizing<Vec<u8>>),
    Resize(u16, u16),
    Gap(Option<u64>),
}
pub(super) struct OwnedFrame {
    occurred_at_ms: i64,
    kind: OwnedFrameKind,
    // Moves through pending batches and writer segments; released only after
    // publication or explicit discard, never when a queue slot is recycled.
    _reservation: Option<Reservation>,
}
struct Segment {
    recording_id: Option<String>,
    frames: Vec<OwnedFrame>,
    bytes: usize,
    last_time: i64,
    last_grid: u32,
    last_flush: Instant,
}
impl Segment {
    fn new() -> Self {
        Self {
            recording_id: None,
            frames: Vec::new(),
            bytes: 0,
            last_time: 0,
            last_grid: 0,
            last_flush: Instant::now(),
        }
    }
    fn push(&mut self, mut frame: OwnedFrame) {
        frame.occurred_at_ms = frame.occurred_at_ms.max(self.last_time);
        self.last_time = frame.occurred_at_ms;
        self.bytes += match &frame.kind {
            OwnedFrameKind::Output(data) => data.len() + 13,
            _ => 24,
        };
        self.frames.push(frame);
    }
}

pub(super) struct Recorder {
    runtime: Arc<RecordingRuntime>,
    segments: HashMap<u64, Segment>,
    buffered: usize,
    health: Arc<Mutex<HealthState>>,
    last_prune: Instant,
}
impl Recorder {
    pub fn new(runtime: Arc<RecordingRuntime>, health: Arc<Mutex<HealthState>>) -> Self {
        Self {
            runtime,
            segments: HashMap::new(),
            buffered: 0,
            health,
            last_prune: Instant::now() - PRUNE_INTERVAL,
        }
    }

    fn received(&self, message: &RecordingMessage) {
        self.runtime.output_queued.fetch_sub(1, Ordering::AcqRel);
        let id = match message {
            RecordingMessage::Batch { sink_id, .. } | RecordingMessage::Resize { sink_id, .. } => {
                sink_id
            }
        };
        if let Some(sink) = self
            .runtime
            .sinks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
        {
            sink.queued.fetch_sub(1, Ordering::AcqRel);
        }
    }

    pub fn handle_queued(
        &mut self,
        message: RecordingMessage,
        store: &mut Result<AuditStore, AuditError>,
    ) {
        self.received(&message);
        self.handle(message, store);
        self.wake_waiters();
    }

    pub fn wake_waiters(&self) {
        self.runtime.wake_waiters();
    }

    pub fn policy(&mut self, store: &mut Result<AuditStore, AuditError>) {
        let enabled = store
            .as_ref()
            .ok()
            .and_then(|store| store.policy().ok())
            .is_some_and(|policy| policy.enabled && policy.record_output);
        let previous = self.runtime.enabled.swap(enabled, Ordering::AcqRel);
        if previous != enabled {
            self.runtime.generation.fetch_add(1, Ordering::AcqRel);
            self.wake_waiters();
        }
        if previous && !enabled {
            for segment in self.segments.values_mut() {
                segment.frames = Vec::new();
                segment.bytes = 0;
                if let (Ok(store), Some(id)) = (store.as_mut(), segment.recording_id.take()) {
                    let _ = store.finish_recording(&id, crate::model::now_ms(), true);
                }
            }
            self.segments.clear();
            self.buffered = 0;
            let sinks = self
                .runtime
                .sinks
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .values()
                .cloned()
                .collect::<Vec<_>>();
            for sink in sinks {
                sink.clear_pending();
                sink.lost_bytes.store(0, Ordering::Release);
                sink.unknown_gap.store(false, Ordering::Release);
                let missed = sink.missed_events.swap(0, Ordering::AcqRel);
                if missed > 0 {
                    self.health
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .missed(AuditError::QueueFull, missed);
                }
            }
        }
    }

    pub fn handle(
        &mut self,
        message: RecordingMessage,
        store: &mut Result<AuditStore, AuditError>,
    ) {
        if !self.runtime.enabled.load(Ordering::Acquire) {
            return;
        }
        let (id, frames, grid) = match message {
            RecordingMessage::Batch {
                sink_id,
                frames,
                grid,
            } => (sink_id, frames, grid),
            RecordingMessage::Resize {
                sink_id,
                occurred_at_ms,
                columns,
                rows,
                reservation,
            } => (
                sink_id,
                vec![OwnedFrame {
                    occurred_at_ms,
                    kind: OwnedFrameKind::Resize(columns, rows),
                    _reservation: Some(reservation),
                }],
                (u32::from(columns) << 16) | u32::from(rows),
            ),
        };
        let sink = {
            self.runtime
                .sinks
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .get(&id)
                .cloned()
        };
        let Some(_sink) = sink else {
            return;
        };
        for frame in frames {
            let segment = self.segments.entry(id).or_insert_with(Segment::new);
            if segment.frames.is_empty() && segment.recording_id.is_none() {
                if grid != 0 && !matches!(frame.kind, OwnedFrameKind::Resize(..)) {
                    let before = segment.bytes;
                    segment.push(OwnedFrame {
                        occurred_at_ms: frame.occurred_at_ms,
                        kind: OwnedFrameKind::Resize((grid >> 16) as u16, grid as u16),
                        _reservation: None,
                    });
                    self.buffered += segment.bytes - before;
                }
                segment.last_grid = grid;
            }
            let before = segment.bytes;
            segment.push(frame);
            self.buffered += segment.bytes - before;
            if segment.frames.len() >= 1000 || self.buffered >= GLOBAL_BUFFER_BYTES {
                self.flush_all(store);
            }
        }
    }

    pub fn tick(&mut self, store: &mut Result<AuditStore, AuditError>) {
        self.policy(store);
        if self.last_prune.elapsed() >= PRUNE_INTERVAL {
            self.last_prune = Instant::now();
            if let Ok(store) = store {
                match store.prune_recordings(crate::model::now_ms()) {
                    Ok(count) => {
                        if count == crate::store::PRUNE_BATCH_CHUNKS {
                            self.last_prune = Instant::now() - PRUNE_INTERVAL;
                        }
                        self.health
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .status
                            .expired += count as u64
                    }
                    Err(error) => {
                        self.health
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .status
                            .error = Some(error)
                    }
                }
            }
        }
        if self.runtime.enabled.load(Ordering::Acquire) {
            let _ = self.drain_pending(store, false);
            self.drain_gaps(store);
            if self.segments.values().any(|segment| {
                !segment.frames.is_empty() && segment.last_flush.elapsed() >= FLUSH_INTERVAL
            }) {
                self.flush_all(store);
            }
        }
        {
            let closed = {
                let registry = self.runtime.sinks.lock().unwrap_or_else(|e| e.into_inner());
                registry
                    .iter()
                    .filter_map(|(&id, sink)| {
                        (sink.closed.load(Ordering::Acquire)
                            && sink.queued.load(Ordering::Acquire) == 0
                            && sink.in_flight.load(Ordering::Acquire) == 0
                            && sink.gate.try_lock().is_ok_and(|pending| pending.bytes == 0))
                        .then(|| (id, sink.clone()))
                    })
                    .collect::<Vec<_>>()
            };
            for (id, sink) in closed {
                if let Some(mut segment) = self.segments.remove(&id) {
                    self.flush_segment(store, &sink, &mut segment);
                    if let (Ok(store), Some(recording_id)) = (store.as_mut(), segment.recording_id)
                    {
                        let _ = store.finish_recording(
                            &recording_id,
                            sink.now_ms(),
                            sink.interrupted.load(Ordering::Acquire),
                        );
                    }
                }
                self.runtime
                    .sinks
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
            }
        }
    }

    pub fn shutdown(&mut self, store: &mut Result<AuditStore, AuditError>) {
        let _ = self.drain_pending(store, true);
        self.drain_gaps(store);
        self.flush_all(store);
        for (&sink_id, segment) in self.segments.iter_mut() {
            if let (Ok(store), Some(id)) = (store.as_mut(), segment.recording_id.take()) {
                let sink = self
                    .runtime
                    .sinks
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&sink_id)
                    .cloned();
                let interrupted = sink.as_ref().is_none_or(|sink| {
                    !sink.closed.load(Ordering::Acquire) || sink.interrupted.load(Ordering::Acquire)
                });
                let ended = sink
                    .as_ref()
                    .map_or_else(crate::model::now_ms, |sink| sink.now_ms());
                let _ = store.finish_recording(&id, ended, interrupted);
            }
        }
    }

    pub fn discard_queued(&self, message: RecordingMessage) {
        self.received(&message);
        let (id, lost) = match message {
            RecordingMessage::Batch {
                sink_id, frames, ..
            } => (
                sink_id,
                Some(
                    frames
                        .iter()
                        .map(|frame| match &frame.kind {
                            OwnedFrameKind::Output(bytes) => bytes.len() as u64,
                            _ => 0,
                        })
                        .sum(),
                ),
            ),
            RecordingMessage::Resize { sink_id, .. } => (sink_id, None),
        };
        if let Some(sink) = self
            .runtime
            .sinks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
        {
            if let Some(bytes) = lost {
                sink.lost_bytes.fetch_add(bytes, Ordering::AcqRel);
            } else {
                sink.unknown_gap.store(true, Ordering::Release);
            }
        }
    }

    pub fn flush_before_policy_change(&mut self, store: &mut Result<AuditStore, AuditError>) {
        let _ = self.drain_pending(store, true);
        self.drain_gaps(store);
        self.flush_all(store);
    }

    fn drain_pending(
        &mut self,
        store: &mut Result<AuditStore, AuditError>,
        blocking: bool,
    ) -> bool {
        let sinks = self
            .runtime
            .sinks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut all_drained = true;
        for sink in sinks {
            if sink.queued.load(Ordering::Acquire) > 0 {
                all_drained = false;
                continue;
            }
            if sink.in_flight.load(Ordering::Acquire) > 0 {
                all_drained = false;
            }
            let message = if blocking {
                let mut pending = sink.gate.lock().unwrap_or_else(|error| error.into_inner());
                sink.take_pending(&mut pending)
            } else if let Ok(mut pending) = sink.gate.try_lock() {
                sink.take_pending(&mut pending)
            } else {
                all_drained = false;
                None
            };
            if let Some(message) = message {
                self.handle(message, store);
            }
        }
        all_drained
    }

    fn drain_gaps(&mut self, store: &mut Result<AuditStore, AuditError>) {
        let sinks = {
            self.runtime
                .sinks
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .values()
                .cloned()
                .collect::<Vec<_>>()
        };
        for sink in sinks {
            if sink.queued.load(Ordering::Acquire) > 0 || sink.in_flight.load(Ordering::Acquire) > 0
            {
                continue;
            }
            let missed = sink.missed_events.swap(0, Ordering::AcqRel);
            if missed > 0 {
                self.health
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .missed(AuditError::QueueFull, missed);
            }
            let lost = sink.lost_bytes.load(Ordering::Acquire);
            let unknown = sink.unknown_gap.load(Ordering::Acquire);
            if lost > 0 || unknown {
                let segment = self.segments.entry(sink.id).or_insert_with(Segment::new);
                if segment.frames.is_empty() && segment.recording_id.is_none() {
                    let grid = sink.grid.load(Ordering::Acquire);
                    if grid != 0 {
                        let before = segment.bytes;
                        segment.push(OwnedFrame {
                            occurred_at_ms: sink.now_ms(),
                            kind: OwnedFrameKind::Resize((grid >> 16) as u16, grid as u16),
                            _reservation: None,
                        });
                        self.buffered += segment.bytes - before;
                    }
                }
                self.buffered += take_gap(&sink, segment, sink.now_ms());
                if self.buffered >= GLOBAL_BUFFER_BYTES {
                    self.flush_all(store);
                }
            }
        }
    }

    fn flush_all(&mut self, store: &mut Result<AuditStore, AuditError>) {
        let ids = self.segments.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let sink = {
                self.runtime
                    .sinks
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&id)
                    .cloned()
            };
            if let Some(sink) = sink {
                if let Some(mut segment) = self.segments.remove(&id) {
                    self.flush_segment(store, &sink, &mut segment);
                    self.segments.insert(id, segment);
                }
            }
        }
    }

    fn flush_segment(
        &mut self,
        store: &mut Result<AuditStore, AuditError>,
        sink: &SinkState,
        segment: &mut Segment,
    ) {
        if segment.frames.is_empty() {
            return;
        }
        self.buffered = self.buffered.saturating_sub(segment.bytes);
        let Some(writer) = store.as_mut().ok() else {
            segment.frames = Vec::new();
            segment.bytes = 0;
            sink.unknown_gap.store(true, Ordering::Release);
            return;
        };
        if segment.recording_id.is_none() {
            match writer.create_recording(
                &sink.instance_id,
                segment.frames[0].occurred_at_ms,
                &sink.details,
            ) {
                Ok(id) => segment.recording_id = id,
                Err(error) => {
                    self.health
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .missed(error, 1);
                    sink.unknown_gap.store(true, Ordering::Release);
                }
            }
        }
        if let Some(id) = &segment.recording_id {
            let views = segment
                .frames
                .iter()
                .map(|frame| RecordingFrame {
                    occurred_at_ms: frame.occurred_at_ms,
                    kind: match &frame.kind {
                        OwnedFrameKind::Output(data) => RecordingFrameKind::Output(data),
                        OwnedFrameKind::Resize(columns, rows) => RecordingFrameKind::Resize {
                            columns: *columns,
                            rows: *rows,
                        },
                        OwnedFrameKind::Gap(lost_bytes) => RecordingFrameKind::Gap {
                            lost_bytes: *lost_bytes,
                        },
                    },
                })
                .collect::<Vec<_>>();
            if let Err(error) = writer.append_recording_chunk(id, &views) {
                self.health
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .missed(error, 1);
                sink.unknown_gap.store(true, Ordering::Release);
                if error == AuditError::Closed {
                    let _ = writer.finish_recording(id, crate::model::now_ms(), true);
                    segment.recording_id = None;
                }
            }
        }
        segment.frames = Vec::new();
        segment.bytes = 0;
        segment.last_flush = Instant::now();
    }
}

fn take_gap(sink: &SinkState, segment: &mut Segment, at: i64) -> usize {
    let lost = sink.lost_bytes.swap(0, Ordering::AcqRel);
    let unknown = sink.unknown_gap.swap(false, Ordering::AcqRel);
    if lost == 0 && !unknown {
        return 0;
    }
    let before = segment.bytes;
    segment.push(OwnedFrame {
        occurred_at_ms: at,
        kind: OwnedFrameKind::Gap(if unknown { None } else { Some(lost) }),
        _reservation: None,
    });
    segment.bytes - before
}
