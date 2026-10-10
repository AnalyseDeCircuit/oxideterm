use parking_lot::Mutex;
use std::{
    any::Any,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// Completion of an actual renderer submission, including retained upload resources.
#[derive(Clone)]
pub struct GpuSubmission(Arc<SubmissionState>);

struct SubmissionState {
    complete: AtomicBool,
    retained: Mutex<Vec<Box<dyn Any + Send>>>,
    started: Instant,
    elapsed_nanos: AtomicU64,
}

impl Default for GpuSubmission {
    fn default() -> Self {
        Self(Arc::new(SubmissionState {
            complete: AtomicBool::new(false),
            retained: Mutex::new(Vec::new()),
            started: Instant::now(),
            elapsed_nanos: AtomicU64::new(0),
        }))
    }
}

impl GpuSubmission {
    /// May be checked without waiting for the GPU on the UI thread.
    pub fn is_complete(&self) -> bool {
        self.0.complete.load(Ordering::Acquire)
    }

    /// Time until completion was observed, including queueing; not a hardware GPU timestamp.
    pub fn elapsed(&self) -> Option<Duration> {
        self.is_complete()
            .then(|| Duration::from_nanos(self.0.elapsed_nanos.load(Ordering::Relaxed)))
    }

    /// Keep admission leases or upload storage alive through device completion.
    pub fn retain(&self, resource: impl Any + Send) {
        let mut retained = self.0.retained.lock();
        if !self.is_complete() {
            retained.push(Box::new(resource));
        }
    }

    /// Called by platform renderers after the submitted work completes.
    pub fn complete(&self) {
        let mut retained = self.0.retained.lock();
        if self.is_complete() {
            return;
        }
        self.0.elapsed_nanos.store(
            self.0.started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
        self.0.complete.store(true, Ordering::Release);
        retained.clear();
    }
}
