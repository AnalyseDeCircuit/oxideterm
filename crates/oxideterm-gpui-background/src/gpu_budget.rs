use oxideterm_background_media::MediaError;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

static GPU_BYTES: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Default)]
pub(crate) struct GpuBudget(Arc<AtomicUsize>);

pub(crate) struct GpuLease {
    budget: GpuBudget,
    bytes: usize,
}

impl GpuBudget {
    pub fn reserve(&self, bytes: usize) -> Result<GpuLease, MediaError> {
        let reserve = |counter: &AtomicUsize, limit| {
            counter
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    current.checked_add(bytes).filter(|next| *next <= limit)
                })
                .is_ok()
        };
        if !reserve(&self.0, 64 * 1024 * 1024) {
            return Err(MediaError::ResourceExhausted);
        }
        if !reserve(&GPU_BYTES, 128 * 1024 * 1024) {
            self.0.fetch_sub(bytes, Ordering::AcqRel);
            return Err(MediaError::ResourceExhausted);
        }
        Ok(GpuLease {
            budget: self.clone(),
            bytes,
        })
    }
}

impl Drop for GpuLease {
    fn drop(&mut self) {
        self.budget.0.fetch_sub(self.bytes, Ordering::AcqRel);
        GPU_BYTES.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
