use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

// This replaces the separate producer/queue/writer payload allowances without
// increasing their combined ceiling. One stream cannot occupy the whole pool.
pub(super) const MAX_RECORDING_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_STREAM_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct Usage {
    bytes: usize,
    streams: HashMap<u64, usize>,
    waiting: VecDeque<u64>,
}

#[derive(Default)]
pub(super) struct RecordingBudget(Mutex<Usage>);

pub(in crate::service) struct Reservation {
    budget: Arc<RecordingBudget>,
    stream: u64,
    bytes: usize,
}

impl RecordingBudget {
    pub fn reserve(self: &Arc<Self>, stream: u64, bytes: usize) -> Option<Reservation> {
        let mut usage = self.0.try_lock().ok()?;
        let stream_bytes = usage.streams.get(&stream).copied().unwrap_or(0);
        if bytes > MAX_STREAM_BYTES.saturating_sub(stream_bytes) {
            // A stream at its own limit must not block other admission waiters.
            usage.waiting.retain(|id| *id != stream);
            return None;
        }
        if usage.waiting.front().is_some_and(|id| *id != stream)
            || bytes > MAX_RECORDING_BYTES.saturating_sub(usage.bytes)
        {
            if !usage.waiting.contains(&stream) {
                usage.waiting.push_back(stream);
            }
            return None;
        }
        if usage.waiting.front() == Some(&stream) {
            usage.waiting.pop_front();
        }
        usage.bytes += bytes;
        usage.streams.insert(stream, stream_bytes + bytes);
        Some(Reservation {
            budget: self.clone(),
            stream,
            bytes,
        })
    }

    pub fn cancel_waiter(&self, stream: u64) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .waiting
            .retain(|id| *id != stream);
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut usage = self.budget.0.lock().unwrap_or_else(|e| e.into_inner());
        usage.bytes -= self.bytes;
        let remaining = usage
            .streams
            .get_mut(&self.stream)
            .expect("reserved recording stream");
        *remaining -= self.bytes;
        if *remaining == 0 {
            usage.streams.remove(&self.stream);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_admission_rotates_waiters_and_cancel_does_not_stall_the_next_stream() {
        let budget = Arc::new(RecordingBudget::default());
        let mut held = (0..4)
            .map(|id| budget.reserve(id, MAX_STREAM_BYTES).unwrap())
            .collect::<Vec<_>>();
        assert!(budget.reserve(4, 32 * 1024).is_none());
        assert!(budget.reserve(5, 32 * 1024).is_none());
        drop(held.pop());
        assert!(
            budget.reserve(6, 32 * 1024).is_none(),
            "new work overtook an existing waiter"
        );
        let first = budget.reserve(4, 32 * 1024).unwrap();
        assert!(
            budget.reserve(4, 32 * 1024).is_none(),
            "one stream took two turns ahead of a waiter"
        );
        budget.cancel_waiter(5);
        let next = budget.reserve(6, 32 * 1024).unwrap();
        let last = budget.reserve(4, 32 * 1024).unwrap();
        drop((first, next, last, held));
        let restored = (0..4)
            .map(|id| budget.reserve(id, MAX_STREAM_BYTES).unwrap())
            .collect::<Vec<_>>();
        assert!(budget.reserve(7, 1).is_none());
        drop(restored);
        assert!(budget.reserve(7, 1).is_some());
    }
}
