use std::sync::atomic::{AtomicUsize, Ordering};

// Terminal graphics and blurred backgrounds share one process-wide admission budget so opening
// more panes cannot multiply the configured per-pane cache allowance without bound.
const GLOBAL_IMAGE_CACHE_BYTES: usize = 256 * 1024 * 1024;
static RESERVED_IMAGE_CACHE_BYTES: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn try_reserve_image_bytes(bytes: usize) -> bool {
    if bytes == 0 || bytes > GLOBAL_IMAGE_CACHE_BYTES {
        return bytes == 0;
    }
    let mut current = RESERVED_IMAGE_CACHE_BYTES.load(Ordering::Acquire);
    loop {
        let Some(next) = current.checked_add(bytes) else {
            return false;
        };
        if next > GLOBAL_IMAGE_CACHE_BYTES {
            return false;
        }
        match RESERVED_IMAGE_CACHE_BYTES.compare_exchange_weak(
            current,
            next,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return true,
            Err(observed) => current = observed,
        }
    }
}

pub(crate) fn release_image_bytes(bytes: usize) {
    if bytes > 0 {
        RESERVED_IMAGE_CACHE_BYTES.fetch_sub(bytes, Ordering::AcqRel);
    }
}
