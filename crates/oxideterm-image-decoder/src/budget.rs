use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const CPU_IMAGE_BYTES: usize = 512 * 1024 * 1024;
static IMAGES: ImageBudget = ImageBudget {
    used: AtomicUsize::new(0),
    limit: CPU_IMAGE_BYTES,
};

struct ImageBudget {
    used: AtomicUsize,
    limit: usize,
}

impl ImageBudget {
    fn reserve(&self, bytes: usize) -> bool {
        self.used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= self.limit)
            })
            .is_ok()
    }

    fn lease(&'static self, bytes: usize) -> Option<ImageMemoryLease> {
        self.reserve(bytes).then(|| {
            ImageMemoryLease(Arc::new(Reservation {
                budget: self,
                bytes,
            }))
        })
    }
}

/// Process-wide allowance shared by protocol pixels, terminal uploads and backgrounds.
pub fn try_reserve_image_bytes(bytes: usize) -> bool {
    IMAGES.reserve(bytes)
}

pub fn release_image_bytes(bytes: usize) {
    IMAGES.used.fetch_sub(bytes, Ordering::AcqRel);
}

pub fn reserve_image_memory(bytes: usize) -> Option<ImageMemoryLease> {
    IMAGES.lease(bytes)
}

/// Clones share one reservation; the final pixel owner releases it.
#[derive(Clone)]
pub struct ImageMemoryLease(Arc<Reservation>);

struct Reservation {
    budget: &'static ImageBudget,
    bytes: usize,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

impl std::fmt::Debug for ImageMemoryLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageMemoryLease")
            .field("bytes", &self.0.bytes)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_pixel_owners_hold_the_allowance_until_the_last_drop() {
        static BUDGET: ImageBudget = ImageBudget {
            used: AtomicUsize::new(0),
            limit: 16,
        };
        let first = BUDGET.lease(12).unwrap();
        let snapshot = first.clone();
        assert!(BUDGET.lease(5).is_none());
        drop(first);
        assert!(BUDGET.lease(5).is_none());
        drop(snapshot);
        let next = BUDGET.lease(16).unwrap();
        assert!(BUDGET.lease(1).is_none());
        drop(next);
        assert_eq!(BUDGET.used.load(Ordering::Acquire), 0);
    }
}
