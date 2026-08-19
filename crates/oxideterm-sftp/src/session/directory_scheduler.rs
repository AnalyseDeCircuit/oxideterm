// Keep directory scheduling policy inside oxideterm-sftp. The protocol crate
// only exposes capacity hints; this layer decides how aggressively to use them.
const DIRECTORY_COMPACT_FILE_MAX_BYTES: u64 = 512 * 1024;
// Reserve a few remote handles for directory reads, metadata probes, and user
// actions that may share the same SFTP subsystem while a transfer is running.
const DIRECTORY_HANDLE_HEADROOM: u64 = 8;
const DIRECTORY_BULK_LANE_WORKERS: usize = 2;
const DIRECTORY_AUX_CHANNEL_LIMIT: usize = 4;
const DIRECTORY_QUEUE_WORKER_MULTIPLIER: usize = 2;
const DIRECTORY_RATE_LIMIT_BURST: std::time::Duration =
    std::time::Duration::from_millis(250);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DirectoryJobClass {
    Compact,
    Bulk,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DirectoryTransferPlan {
    worker_count: usize,
    channel_count: usize,
    bulk_lane_workers: usize,
    compact_file_max_bytes: u64,
    queue_capacity: usize,
}

impl DirectoryTransferPlan {
    fn classify_size(&self, bytes: u64) -> DirectoryJobClass {
        if bytes > self.compact_file_max_bytes {
            DirectoryJobClass::Bulk
        } else {
            DirectoryJobClass::Compact
        }
    }
}

#[derive(Debug)]
struct DirectoryRateLimiter {
    state: parking_lot::Mutex<DirectoryRateLimiterState>,
}

#[derive(Debug)]
struct DirectoryRateLimiterState {
    limit_bps: usize,
    theoretical_arrival: Instant,
}

impl DirectoryRateLimiter {
    fn new() -> Self {
        Self {
            state: parking_lot::Mutex::new(DirectoryRateLimiterState {
                limit_bps: 0,
                theoretical_arrival: Instant::now(),
            }),
        }
    }

    async fn throttle(
        &self,
        bytes: usize,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> std::time::Duration {
        let limit_bps = transfer_manager
            .as_ref()
            .map(|manager| manager.speed_limit_bps())
            .unwrap_or(0);
        let delay = self.reserve_delay_at(bytes, limit_bps, Instant::now());
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        delay
    }

    fn reserve_delay_at(
        &self,
        bytes: usize,
        limit_bps: usize,
        now: Instant,
    ) -> std::time::Duration {
        let mut state = self.state.lock();
        if limit_bps == 0 {
            state.limit_bps = 0;
            state.theoretical_arrival = now;
            return std::time::Duration::ZERO;
        }
        if state.limit_bps != limit_bps {
            // A live rate change starts a fresh token window so bytes sent under
            // the previous setting never create an unexpected catch-up delay.
            state.limit_bps = limit_bps;
            state.theoretical_arrival = now;
        }

        let service_time =
            std::time::Duration::from_secs_f64(bytes as f64 / limit_bps as f64);
        let reservation_start = state.theoretical_arrival.max(now);
        let reservation_end = reservation_start + service_time;
        state.theoretical_arrival = reservation_end;

        // This virtual-scheduling form is equivalent to a shared token bucket:
        // all workers consume one batch budget while retaining a small burst.
        reservation_end
            .checked_sub(DIRECTORY_RATE_LIMIT_BURST)
            .unwrap_or(now)
            .saturating_duration_since(now)
    }
}

fn plan_directory_transfer(
    requested_parallelism: usize,
    advertised_open_handle_limit: Option<u64>,
) -> DirectoryTransferPlan {
    let requested_workers =
        requested_parallelism.clamp(1, crate::MAX_SFTP_DIRECTORY_PARALLELISM);
    let handle_workers = advertised_open_handle_limit
        .map(|limit| limit.saturating_sub(DIRECTORY_HANDLE_HEADROOM).max(1) as usize)
        .unwrap_or(crate::MAX_SFTP_DIRECTORY_PARALLELISM);
    let worker_count = requested_workers.min(handle_workers).max(1);
    let channel_count = worker_count.min(DIRECTORY_AUX_CHANNEL_LIMIT).max(1);
    let bulk_lane_workers = worker_count.min(DIRECTORY_BULK_LANE_WORKERS).max(1);
    let queue_capacity = worker_count
        .saturating_mul(DIRECTORY_QUEUE_WORKER_MULTIPLIER)
        .max(1);

    DirectoryTransferPlan {
        worker_count,
        channel_count,
        bulk_lane_workers,
        compact_file_max_bytes: DIRECTORY_COMPACT_FILE_MAX_BYTES,
        queue_capacity,
    }
}

fn directory_job_channel<T>(
    plan: DirectoryTransferPlan,
) -> (
    tokio::sync::mpsc::Sender<T>,
    tokio::sync::mpsc::Receiver<T>,
) {
    // The queue is deliberately bounded so enumeration cannot outrun disk,
    // network, and remote-handle capacity on very large directory trees.
    tokio::sync::mpsc::channel(plan.queue_capacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_bounds_parallelism_by_request_and_server_capacity() {
        for (requested, server_limit, workers, channels) in [
            (8, None, 8, 4),
            (16, Some(10), 2, 2),
            (16, Some(1), 1, 1),
            (16, None, 16, 4),
        ] {
            let plan = plan_directory_transfer(requested, server_limit);
            assert_eq!((plan.worker_count, plan.channel_count), (workers, channels));
        }
    }

    #[test]
    fn shared_rate_limiter_reserves_one_batch_budget() {
        let limiter = DirectoryRateLimiter::new();
        let now = Instant::now();
        let bytes_per_second = 64 * 1024;

        let first = limiter.reserve_delay_at(bytes_per_second, bytes_per_second, now);
        let second = limiter.reserve_delay_at(bytes_per_second, bytes_per_second, now);

        assert_eq!(first, std::time::Duration::from_millis(750));
        assert_eq!(second, std::time::Duration::from_millis(1_750));
    }

    #[test]
    fn shared_rate_limiter_resets_when_limit_changes() {
        let limiter = DirectoryRateLimiter::new();
        let now = Instant::now();
        let _ = limiter.reserve_delay_at(64 * 1024, 64 * 1024, now);

        let changed = limiter.reserve_delay_at(128 * 1024, 128 * 1024, now);
        let disabled = limiter.reserve_delay_at(128 * 1024, 0, now);

        assert_eq!(changed, std::time::Duration::from_millis(750));
        assert_eq!(disabled, std::time::Duration::ZERO);
    }

    #[test]
    fn planner_classifies_bulk_jobs_by_size() {
        let plan = plan_directory_transfer(3, None);

        assert_eq!(
            plan.classify_size(DIRECTORY_COMPACT_FILE_MAX_BYTES),
            DirectoryJobClass::Compact
        );
        assert_eq!(
            plan.classify_size(DIRECTORY_COMPACT_FILE_MAX_BYTES + 1),
            DirectoryJobClass::Bulk
        );
    }

    #[test]
    fn directory_job_channel_applies_planned_backpressure() {
        let plan = plan_directory_transfer(2, None);
        let (sender, _receiver) = directory_job_channel(plan);

        for job in 0..plan.queue_capacity {
            sender.try_send(job).expect("planned queue slot");
        }
        assert!(matches!(
            sender.try_send(plan.queue_capacity),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_))
        ));
    }
}
