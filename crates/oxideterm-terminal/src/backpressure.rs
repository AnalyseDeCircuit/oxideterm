// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    borrow::Cow,
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::Duration,
};

use tokio::sync::Notify;

pub(crate) const LOCAL_PTY_READ_BUFFER_BYTES: usize = 8 * 1024;
pub(crate) const LOCAL_MAX_LOCKED_PARSE_BYTES: usize = 64 * 1024;
pub(crate) const UTF8_RESIDUAL_MAX_BYTES: usize = 4;

// Keep enough transport output for several normal drains while placing a
// deterministic ceiling on memory retained between a worker and its session.
pub(crate) const TRANSPORT_OUTPUT_BACKLOG_BYTES: usize = 1024 * 1024;

pub const NATIVE_INTERACTIVE_DRAIN_BYTES: usize = 32 * 1024;
pub const NATIVE_NORMAL_DRAIN_BYTES: usize = 128 * 1024;
pub const NATIVE_THROUGHPUT_DRAIN_BYTES: usize = 256 * 1024;

const DEFAULT_MAX_EVENTS_PER_DRAIN: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalDrainBudget {
    pub max_bytes: usize,
    pub max_events: usize,
    pub max_duration: Duration,
    pub collect_performance_metrics: bool,
}

impl TerminalDrainBudget {
    pub const fn new(max_bytes: usize, max_events: usize) -> Self {
        Self {
            max_bytes,
            max_events,
            max_duration: Duration::MAX,
            collect_performance_metrics: false,
        }
    }

    pub const fn with_max_duration(mut self, max_duration: Duration) -> Self {
        self.max_duration = max_duration;
        self
    }

    pub const fn with_performance_metrics(mut self, enabled: bool) -> Self {
        self.collect_performance_metrics = enabled;
        self
    }

    pub fn time_exhausted(self, started: std::time::Instant) -> bool {
        started.elapsed() >= self.max_duration
    }

    pub const fn interactive() -> Self {
        Self::new(NATIVE_INTERACTIVE_DRAIN_BYTES, DEFAULT_MAX_EVENTS_PER_DRAIN)
    }

    pub const fn normal() -> Self {
        Self::new(NATIVE_NORMAL_DRAIN_BYTES, DEFAULT_MAX_EVENTS_PER_DRAIN)
    }

    pub const fn throughput() -> Self {
        Self::new(NATIVE_THROUGHPUT_DRAIN_BYTES, DEFAULT_MAX_EVENTS_PER_DRAIN)
    }

    pub const fn unlimited() -> Self {
        Self::new(usize::MAX, usize::MAX)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalDrainReport {
    pub changed: bool,
    pub drained_bytes: usize,
    pub pending_bytes: usize,
    pub events_drained: usize,
    pub drain_duration: Duration,
    pub max_data_chunk_bytes: usize,
    pub output_processing_duration: Duration,
    pub terminal_lock_wait_duration: Duration,
    pub budget_exhausted: bool,
}

impl TerminalDrainReport {
    pub fn mark_changed(&mut self) {
        self.changed = true;
    }

    pub fn record_data_chunk(&mut self, byte_len: usize, processing_duration: Duration) {
        self.drained_bytes = self.drained_bytes.saturating_add(byte_len);
        self.max_data_chunk_bytes = self.max_data_chunk_bytes.max(byte_len);
        self.output_processing_duration += processing_duration;
    }

    pub fn combine(&mut self, other: TerminalDrainReport) {
        self.changed |= other.changed;
        self.drained_bytes = self.drained_bytes.saturating_add(other.drained_bytes);
        self.pending_bytes = self.pending_bytes.saturating_add(other.pending_bytes);
        self.events_drained = self.events_drained.saturating_add(other.events_drained);
        self.drain_duration += other.drain_duration;
        self.max_data_chunk_bytes = self.max_data_chunk_bytes.max(other.max_data_chunk_bytes);
        self.output_processing_duration += other.output_processing_duration;
        self.terminal_lock_wait_duration += other.terminal_lock_wait_duration;
        self.budget_exhausted |= other.budget_exhausted;
    }
}

struct QueuedOutput<T> {
    value: T,
    byte_len: usize,
}

struct ByteBoundedState<T> {
    queue: VecDeque<QueuedOutput<T>>,
    outstanding_bytes: usize,
    sender_open: bool,
    receiver_open: bool,
}

struct ByteBoundedInner<T> {
    state: Mutex<ByteBoundedState<T>>,
    blocking_space: Condvar,
    async_space: Notify,
    max_bytes: usize,
    activity: Option<crate::activity::TerminalActivitySender>,
}

/// Sends worker events while charging data events against an exact byte limit.
pub(crate) struct ByteBoundedSender<T> {
    inner: Arc<ByteBoundedInner<T>>,
}

/// Receives worker events and exposes the bytes still retained by the channel.
pub(crate) struct ByteBoundedReceiver<T> {
    inner: Arc<ByteBoundedInner<T>>,
}

/// Keeps a data event's byte reservation alive while it waits in a local drain queue.
pub(crate) struct ByteBoundedItem<T> {
    value: Option<T>,
    byte_len: usize,
    inner: Arc<ByteBoundedInner<T>>,
}

#[cfg(test)]
pub(crate) fn byte_bounded_channel<T>(
    max_bytes: usize,
) -> (ByteBoundedSender<T>, ByteBoundedReceiver<T>) {
    byte_bounded_channel_inner(max_bytes, None)
}

pub(crate) fn byte_bounded_channel_with_activity<T>(
    max_bytes: usize,
    activity: crate::activity::TerminalActivitySender,
) -> (ByteBoundedSender<T>, ByteBoundedReceiver<T>) {
    byte_bounded_channel_inner(max_bytes, Some(activity))
}

fn byte_bounded_channel_inner<T>(
    max_bytes: usize,
    activity: Option<crate::activity::TerminalActivitySender>,
) -> (ByteBoundedSender<T>, ByteBoundedReceiver<T>) {
    assert!(max_bytes > 0, "byte-bounded channels need a positive limit");
    let inner = Arc::new(ByteBoundedInner {
        state: Mutex::new(ByteBoundedState {
            queue: VecDeque::new(),
            outstanding_bytes: 0,
            sender_open: true,
            receiver_open: true,
        }),
        blocking_space: Condvar::new(),
        async_space: Notify::new(),
        max_bytes,
        activity,
    });
    (
        ByteBoundedSender {
            inner: Arc::clone(&inner),
        },
        ByteBoundedReceiver { inner },
    )
}

impl<T> ByteBoundedSender<T> {
    /// Blocks a dedicated worker thread until this data event fits the byte budget.
    pub(crate) fn send(&self, value: T, byte_len: usize) -> Result<(), T> {
        if byte_len > self.inner.max_bytes {
            return Err(value);
        }

        let mut value = Some(value);
        let mut state = lock_state(&self.inner);
        loop {
            if !state.receiver_open {
                return Err(value.take().expect("queued value must be present"));
            }
            if state.outstanding_bytes.saturating_add(byte_len) <= self.inner.max_bytes {
                enqueue(
                    &mut state,
                    value.take().expect("queued value must be present"),
                    byte_len,
                );
                drop(state);
                if let Some(activity) = &self.inner.activity {
                    activity.notify();
                }
                return Ok(());
            }
            state = self
                .inner
                .blocking_space
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    /// Awaits byte capacity without blocking a Tokio runtime worker thread.
    pub(crate) async fn send_async(&self, value: T, byte_len: usize) -> Result<(), T> {
        if byte_len > self.inner.max_bytes {
            return Err(value);
        }

        let mut value = Some(value);
        loop {
            // Notify stores one permit when the receiver frees space before
            // this future starts waiting, which closes the check/wait race.
            let space_available = self.inner.async_space.notified();
            {
                let mut state = lock_state(&self.inner);
                if !state.receiver_open {
                    return Err(value.take().expect("queued value must be present"));
                }
                if state.outstanding_bytes.saturating_add(byte_len) <= self.inner.max_bytes {
                    enqueue(
                        &mut state,
                        value.take().expect("queued value must be present"),
                        byte_len,
                    );
                    drop(state);
                    if let Some(activity) = &self.inner.activity {
                        activity.notify();
                    }
                    return Ok(());
                }
            }
            space_available.await;
        }
    }

    /// Control events bypass the data-byte limit so lifecycle state is never dropped.
    pub(crate) fn send_control(&self, value: T) -> Result<(), T> {
        let mut state = lock_state(&self.inner);
        if !state.receiver_open {
            return Err(value);
        }
        enqueue(&mut state, value, 0);
        drop(state);
        if let Some(activity) = &self.inner.activity {
            activity.notify();
        }
        Ok(())
    }
}

impl<T> Drop for ByteBoundedSender<T> {
    fn drop(&mut self) {
        let mut state = lock_state(&self.inner);
        state.sender_open = false;
        drop(state);
        // A worker can exit without publishing a final control event; wake the owner so it can
        // observe the disconnected queue instead of sleeping until an unrelated deadline.
        if let Some(activity) = &self.inner.activity {
            activity.notify();
        }
    }
}

impl<T> ByteBoundedReceiver<T> {
    pub(crate) fn try_recv(&self) -> Result<ByteBoundedItem<T>, crossbeam_channel::TryRecvError> {
        let mut state = lock_state(&self.inner);
        if let Some(queued) = state.queue.pop_front() {
            return Ok(ByteBoundedItem {
                value: Some(queued.value),
                byte_len: queued.byte_len,
                inner: Arc::clone(&self.inner),
            });
        }
        if state.sender_open {
            Err(crossbeam_channel::TryRecvError::Empty)
        } else {
            Err(crossbeam_channel::TryRecvError::Disconnected)
        }
    }

    pub(crate) fn pending_bytes(&self) -> usize {
        lock_state(&self.inner).outstanding_bytes
    }

    pub(crate) fn is_empty(&self) -> bool {
        lock_state(&self.inner).queue.is_empty()
    }

    /// Wakes blocked producers and rejects future sends during session shutdown.
    pub(crate) fn close(&self) {
        let mut state = lock_state(&self.inner);
        if !state.receiver_open {
            return;
        }
        state.receiver_open = false;
        let queued_bytes = state
            .queue
            .drain(..)
            .map(|queued| queued.byte_len)
            .sum::<usize>();
        state.outstanding_bytes = state.outstanding_bytes.saturating_sub(queued_bytes);
        drop(state);
        self.inner.blocking_space.notify_all();
        self.inner.async_space.notify_one();
    }
}

impl<T> Drop for ByteBoundedReceiver<T> {
    fn drop(&mut self) {
        self.close();
    }
}

impl<T> ByteBoundedItem<T> {
    pub(crate) fn value(&self) -> &T {
        self.value.as_ref().expect("received value must be present")
    }

    pub(crate) fn into_inner(mut self) -> T {
        self.value.take().expect("received value must be present")
    }
}

impl<T> Drop for ByteBoundedItem<T> {
    fn drop(&mut self) {
        if self.byte_len == 0 {
            return;
        }
        let mut state = lock_state(&self.inner);
        state.outstanding_bytes = state.outstanding_bytes.saturating_sub(self.byte_len);
        drop(state);
        self.inner.blocking_space.notify_one();
        self.inner.async_space.notify_one();
    }
}

fn enqueue<T>(state: &mut ByteBoundedState<T>, value: T, byte_len: usize) {
    state.outstanding_bytes = state.outstanding_bytes.saturating_add(byte_len);
    state.queue.push_back(QueuedOutput { value, byte_len });
}

fn lock_state<T>(inner: &ByteBoundedInner<T>) -> MutexGuard<'_, ByteBoundedState<T>> {
    inner
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Default)]
pub(crate) struct Utf8ResidualGuard {
    residual: Vec<u8>,
}

impl Utf8ResidualGuard {
    pub(crate) fn push<'a>(&mut self, bytes: &'a [u8]) -> Option<Cow<'a, [u8]>> {
        if bytes.is_empty() && self.residual.is_empty() {
            return None;
        }

        if self.residual.is_empty() {
            let split = split_before_incomplete_utf8_tail(bytes);
            if split < bytes.len() {
                // Retain only the incomplete scalar instead of copying the complete chunk.
                self.residual.extend_from_slice(&bytes[split..]);
            }
            return (split > 0).then(|| Cow::Borrowed(&bytes[..split]));
        }

        let mut combined = Vec::with_capacity(self.residual.len() + bytes.len());
        combined.extend_from_slice(&self.residual);
        combined.extend_from_slice(bytes);
        self.residual.clear();

        let split = split_before_incomplete_utf8_tail(&combined);
        if split < combined.len() {
            self.residual.extend_from_slice(&combined[split..]);
            combined.truncate(split);
        }

        if self.residual.len() >= UTF8_RESIDUAL_MAX_BYTES {
            combined.extend_from_slice(&self.residual);
            self.residual.clear();
        }

        (!combined.is_empty()).then_some(Cow::Owned(combined))
    }

    pub(crate) fn flush(&mut self) -> Option<Vec<u8>> {
        (!self.residual.is_empty()).then(|| std::mem::take(&mut self.residual))
    }
}

fn split_before_incomplete_utf8_tail(bytes: &[u8]) -> usize {
    let len = bytes.len();
    let max_tail = len.min(UTF8_RESIDUAL_MAX_BYTES - 1);

    for tail_len in 1..=max_tail {
        let start = len - tail_len;
        let first = bytes[start];
        let width = utf8_char_width(first);
        if width == 0 {
            continue;
        }

        if width > tail_len
            && bytes[start + 1..]
                .iter()
                .all(|byte| is_utf8_continuation(*byte))
        {
            return start;
        }

        break;
    }

    len
}

fn utf8_char_width(byte: u8) -> usize {
    match byte {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 0,
    }
}

fn is_utf8_continuation(byte: u8) -> bool {
    (0x80..=0xbf).contains(&byte)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalMagicKind {
    TrzszTransfer,
}

impl TerminalMagicKind {
    const fn marker(self) -> &'static [u8] {
        match self {
            Self::TrzszTransfer => b"::TRZSZ:TRANSFER:",
        }
    }
}

#[derive(Debug)]
pub(crate) struct MagicScanWindow {
    tail: Vec<u8>,
    patterns: Vec<TerminalMagicKind>,
}

impl Default for MagicScanWindow {
    fn default() -> Self {
        Self {
            tail: Vec::new(),
            patterns: vec![TerminalMagicKind::TrzszTransfer],
        }
    }
}

impl MagicScanWindow {
    pub(crate) fn scan(&mut self, chunk: &[u8]) -> Vec<TerminalMagicKind> {
        if chunk.is_empty() {
            return Vec::new();
        }

        let mut matches = Vec::new();

        for kind in &self.patterns {
            let marker = kind.marker();
            if marker.is_empty() {
                continue;
            }
            if marker_crosses_chunk_boundary(&self.tail, chunk, marker)
                || chunk_contains_marker(chunk, marker)
            {
                matches.push(*kind);
            }
        }

        let keep = self
            .patterns
            .iter()
            .map(|kind| kind.marker().len().saturating_sub(1))
            .max()
            .unwrap_or(0);
        if chunk.len() >= keep {
            self.tail.clear();
            self.tail.extend_from_slice(&chunk[chunk.len() - keep..]);
        } else {
            self.tail.extend_from_slice(chunk);
            if self.tail.len() > keep {
                let discard = self.tail.len() - keep;
                self.tail.copy_within(discard.., 0);
                self.tail.truncate(keep);
            }
        }
        matches
    }
}

fn marker_crosses_chunk_boundary(tail: &[u8], chunk: &[u8], marker: &[u8]) -> bool {
    let max_tail_bytes = tail.len().min(marker.len().saturating_sub(1));
    (1..=max_tail_bytes).any(|tail_bytes| {
        let chunk_bytes = marker.len() - tail_bytes;
        chunk_bytes <= chunk.len()
            && tail[tail.len() - tail_bytes..] == marker[..tail_bytes]
            && chunk[..chunk_bytes] == marker[tail_bytes..]
    })
}

fn chunk_contains_marker(mut chunk: &[u8], marker: &[u8]) -> bool {
    while let Some(offset) = chunk.iter().position(|byte| *byte == marker[0]) {
        chunk = &chunk[offset..];
        if chunk.starts_with(marker) {
            return true;
        }
        chunk = &chunk[1..];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };

    #[test]
    fn utf8_guard_keeps_incomplete_tail() {
        let mut guard = Utf8ResidualGuard::default();
        assert_eq!(guard.push(&[0xe4, 0xbd]), None);
        assert_eq!(guard.push(&[0xa0]).as_deref(), Some("你".as_bytes()));
    }

    #[test]
    fn utf8_guard_flushes_invalid_bytes_unchanged() {
        let mut guard = Utf8ResidualGuard::default();
        assert_eq!(
            guard.push(&[0xff, b'a']).as_deref(),
            Some(&[0xff, b'a'][..])
        );
    }

    #[test]
    fn utf8_guard_does_not_split_emoji_tail() {
        let mut guard = Utf8ResidualGuard::default();
        assert_eq!(guard.push(&[0xf0, 0x9f, 0x98]), None);
        assert_eq!(guard.push(&[0x80]).as_deref(), Some("😀".as_bytes()));
    }

    #[test]
    fn utf8_guard_borrows_complete_chunks() {
        let mut guard = Utf8ResidualGuard::default();
        let bytes = "plain 中文 🚀".as_bytes();

        assert!(matches!(guard.push(bytes), Some(Cow::Borrowed(value)) if value == bytes));
    }

    #[test]
    fn utf8_guard_flushes_residual_on_stream_end() {
        let mut guard = Utf8ResidualGuard::default();
        assert_eq!(guard.push(&[0xe4, 0xbd]), None);
        assert_eq!(guard.flush(), Some(vec![0xe4, 0xbd]));
        assert_eq!(guard.flush(), None);
    }

    #[test]
    fn magic_scan_detects_split_pattern_once() {
        let mut scan = MagicScanWindow::default();
        assert!(scan.scan(b"abc::TRZSZ:").is_empty());
        assert_eq!(scan.scan(b"TRANSFER:R:1").len(), 1);
        assert!(scan.scan(b"ordinary output").is_empty());
    }

    #[test]
    fn magic_scan_detects_every_cross_chunk_split() {
        let marker = TerminalMagicKind::TrzszTransfer.marker();
        for split in 1..marker.len() {
            let mut scan = MagicScanWindow::default();
            assert!(scan.scan(&marker[..split]).is_empty(), "split {split}");
            assert_eq!(scan.scan(&marker[split..]).len(), 1, "split {split}");
        }
    }

    #[test]
    fn magic_scan_skips_false_marker_prefixes_in_current_chunk() {
        let mut scan = MagicScanWindow::default();
        assert_eq!(
            scan.scan(b"time:12:34 status:ok ::TRZSZ:TRANSFER:R:1")
                .len(),
            1
        );
    }

    #[test]
    fn blocking_sender_stops_at_byte_limit_until_slow_consumer_drains() {
        const LIMIT_BYTES: usize = 64 * 1024;
        const CHUNK_BYTES: usize = 4 * 1024;
        const CHUNK_COUNT: usize = 256;

        let (sender, receiver) = byte_bounded_channel(LIMIT_BYTES);
        let sent_chunks = Arc::new(AtomicUsize::new(0));
        let producer_progress = Arc::clone(&sent_chunks);
        let (finished_tx, finished_rx) = mpsc::channel();
        let producer = std::thread::spawn(move || {
            for _ in 0..CHUNK_COUNT {
                sender.send(vec![0_u8; CHUNK_BYTES], CHUNK_BYTES).unwrap();
                producer_progress.fetch_add(1, Ordering::Release);
            }
            finished_tx.send(()).unwrap();
        });

        let full_chunk_count = LIMIT_BYTES / CHUNK_BYTES;
        let fill_deadline = std::time::Instant::now() + Duration::from_secs(1);
        while sent_chunks.load(Ordering::Acquire) < full_chunk_count {
            assert!(std::time::Instant::now() < fill_deadline);
            std::thread::yield_now();
        }

        assert_eq!(receiver.pending_bytes(), LIMIT_BYTES);
        assert!(finished_rx.recv_timeout(Duration::from_millis(50)).is_err());
        assert_eq!(sent_chunks.load(Ordering::Acquire), full_chunk_count);

        let mut received_chunks = 0;
        while received_chunks < CHUNK_COUNT {
            match receiver.try_recv() {
                Ok(item) => {
                    drop(item);
                    received_chunks += 1;
                    assert!(receiver.pending_bytes() <= LIMIT_BYTES);
                }
                Err(crossbeam_channel::TryRecvError::Empty) => std::thread::yield_now(),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    panic!("producer disconnected before all output was received")
                }
            }
        }

        finished_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        producer.join().unwrap();
        assert_eq!(receiver.pending_bytes(), 0);
    }

    #[test]
    fn closing_receiver_releases_blocked_sender() {
        const LIMIT_BYTES: usize = 8;
        let (sender, receiver) = byte_bounded_channel(LIMIT_BYTES);
        sender.send(vec![1_u8; LIMIT_BYTES], LIMIT_BYTES).unwrap();
        let (finished_tx, finished_rx) = mpsc::channel();
        let blocked_sender = std::thread::spawn(move || {
            let result = sender.send(vec![2_u8; LIMIT_BYTES], LIMIT_BYTES);
            finished_tx.send(result.is_err()).unwrap();
        });

        assert!(finished_rx.recv_timeout(Duration::from_millis(50)).is_err());
        receiver.close();

        assert!(finished_rx.recv_timeout(Duration::from_secs(1)).unwrap());
        blocked_sender.join().unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_sender_resumes_and_control_event_survives_full_data_budget() {
        #[derive(Debug, PartialEq, Eq)]
        enum TestEvent {
            Output(Vec<u8>),
            Closed,
        }

        const LIMIT_BYTES: usize = 8;
        let (sender, receiver) = byte_bounded_channel(LIMIT_BYTES);
        sender
            .send_async(TestEvent::Output(vec![1; LIMIT_BYTES]), LIMIT_BYTES)
            .await
            .unwrap();
        sender.send_control(TestEvent::Closed).unwrap();

        let mut blocked_send = tokio::spawn(async move {
            sender
                .send_async(TestEvent::Output(vec![2; LIMIT_BYTES]), LIMIT_BYTES)
                .await
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut blocked_send)
                .await
                .is_err()
        );
        assert_eq!(receiver.pending_bytes(), LIMIT_BYTES);

        assert_eq!(
            receiver.try_recv().unwrap().into_inner(),
            TestEvent::Output(vec![1; LIMIT_BYTES])
        );
        assert_eq!(receiver.try_recv().unwrap().into_inner(), TestEvent::Closed);
        assert!(
            tokio::time::timeout(Duration::from_secs(1), &mut blocked_send)
                .await
                .unwrap()
                .unwrap()
                .is_ok()
        );
        assert_eq!(receiver.pending_bytes(), LIMIT_BYTES);
        assert_eq!(
            receiver.try_recv().unwrap().into_inner(),
            TestEvent::Output(vec![2; LIMIT_BYTES])
        );
        assert_eq!(receiver.pending_bytes(), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sender_drop_wakes_an_activity_driven_consumer() {
        let (activity_sender, activity_receiver) = crate::activity::terminal_activity_channel();
        let (sender, _receiver) = byte_bounded_channel_with_activity::<Vec<u8>>(8, activity_sender);

        drop(sender);

        assert!(
            tokio::time::timeout(Duration::from_secs(1), activity_receiver.notified())
                .await
                .is_ok()
        );
    }
}
