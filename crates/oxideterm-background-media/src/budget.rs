use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const CPU_BYTES: usize = 256 * 1024 * 1024;
const PLAYER_BYTES: usize = 192 * 1024 * 1024;
static RESERVED: AtomicUsize = AtomicUsize::new(0);

fn reserve(counter: &AtomicUsize, bytes: usize, limit: usize) -> bool {
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current.checked_add(bytes).filter(|next| *next <= limit)
        })
        .is_ok()
}

/// Terminal images and backgrounds use the same process-wide CPU allowance.
pub fn try_reserve_image_bytes(bytes: usize) -> bool {
    reserve(&RESERVED, bytes, CPU_BYTES)
}

pub fn release_image_bytes(bytes: usize) {
    RESERVED.fetch_sub(bytes, Ordering::AcqRel);
}

#[derive(Clone)]
pub struct MemoryBudget(Arc<AtomicUsize>);

impl Default for MemoryBudget {
    fn default() -> Self {
        Self(Arc::new(AtomicUsize::new(0)))
    }
}

impl MemoryBudget {
    pub fn reserve(&self, bytes: usize) -> Result<PixelLease, crate::MediaError> {
        if !reserve(&self.0, bytes, PLAYER_BYTES) {
            return Err(crate::MediaError::ResourceExhausted);
        }
        if !try_reserve_image_bytes(bytes) {
            self.0.fetch_sub(bytes, Ordering::AcqRel);
            return Err(crate::MediaError::ResourceExhausted);
        }
        Ok(PixelLease {
            budget: self.clone(),
            bytes,
        })
    }

    pub fn reserved_bytes(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }
}

/// A buffer's admission remains held until its last consumer releases it.
pub struct PixelLease {
    budget: MemoryBudget,
    bytes: usize,
}

impl PixelLease {
    pub(crate) fn budget(&self) -> MemoryBudget {
        self.budget.clone()
    }
}

impl Drop for PixelLease {
    fn drop(&mut self) {
        self.budget.0.fetch_sub(self.bytes, Ordering::AcqRel);
        release_image_bytes(self.bytes);
    }
}
