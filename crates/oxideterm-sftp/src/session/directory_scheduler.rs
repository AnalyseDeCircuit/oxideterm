// Keep directory scheduling policy inside oxideterm-sftp. The protocol crate
// only exposes capacity hints; this layer decides how aggressively to use them.
const DIRECTORY_COMPACT_FILE_MAX_BYTES: u64 = 512 * 1024;
// Reserve a few remote handles for directory reads, metadata probes, and user
// actions that may share the same SFTP subsystem while a transfer is running.
const DIRECTORY_HANDLE_HEADROOM: u64 = 8;
const DIRECTORY_BULK_LANE_WORKERS: usize = 2;
const DIRECTORY_AUX_CHANNEL_LIMIT: usize = 4;

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

fn plan_directory_transfer(
    job_count: usize,
    requested_parallelism: usize,
    speed_limit_bps: usize,
    advertised_open_handle_limit: Option<u64>,
) -> DirectoryTransferPlan {
    if job_count == 0 {
        return DirectoryTransferPlan {
            worker_count: 0,
            channel_count: 0,
            bulk_lane_workers: 0,
            compact_file_max_bytes: DIRECTORY_COMPACT_FILE_MAX_BYTES,
        };
    }

    // Global speed limiting is transfer-wide today. Keep directory work serial
    // until the limiter can fairly coordinate multiple workers.
    let requested_workers = if speed_limit_bps > 0 {
        1
    } else {
        requested_parallelism.clamp(1, crate::MAX_SFTP_DIRECTORY_PARALLELISM)
    };
    let handle_workers = advertised_open_handle_limit
        .map(|limit| limit.saturating_sub(DIRECTORY_HANDLE_HEADROOM).max(1) as usize)
        .unwrap_or(crate::MAX_SFTP_DIRECTORY_PARALLELISM);
    let worker_count = job_count.min(requested_workers).min(handle_workers).max(1);
    let channel_count = worker_count.min(DIRECTORY_AUX_CHANNEL_LIMIT).max(1);
    let bulk_lane_workers = worker_count.min(DIRECTORY_BULK_LANE_WORKERS).max(1);

    DirectoryTransferPlan {
        worker_count,
        channel_count,
        bulk_lane_workers,
        compact_file_max_bytes: DIRECTORY_COMPACT_FILE_MAX_BYTES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_uses_requested_parallelism_when_server_limit_is_unknown() {
        let plan = plan_directory_transfer(100, 8, 0, None);

        assert_eq!(plan.worker_count, 8);
        assert_eq!(plan.channel_count, 4);
        assert_eq!(plan.bulk_lane_workers, 2);
    }

    #[test]
    fn planner_respects_small_server_handle_budget() {
        let plan = plan_directory_transfer(100, 16, 0, Some(10));

        assert_eq!(plan.worker_count, 2);
        assert_eq!(plan.channel_count, 2);
    }

    #[test]
    fn planner_keeps_one_worker_when_handle_budget_is_tiny() {
        let plan = plan_directory_transfer(100, 16, 0, Some(1));

        assert_eq!(plan.worker_count, 1);
        assert_eq!(plan.channel_count, 1);
        assert_eq!(plan.bulk_lane_workers, 1);
    }

    #[test]
    fn planner_disables_directory_parallelism_when_speed_limit_is_enabled() {
        let plan = plan_directory_transfer(100, 16, 64 * 1024, None);

        assert_eq!(plan.worker_count, 1);
        assert_eq!(plan.channel_count, 1);
    }

    #[test]
    fn planner_reports_zero_capacity_for_empty_work() {
        let plan = plan_directory_transfer(0, 8, 0, None);

        assert_eq!(plan.worker_count, 0);
        assert_eq!(plan.channel_count, 0);
        assert_eq!(plan.bulk_lane_workers, 0);
    }

    #[test]
    fn planner_classifies_bulk_jobs_by_size() {
        let plan = plan_directory_transfer(3, 3, 0, None);

        assert_eq!(
            plan.classify_size(DIRECTORY_COMPACT_FILE_MAX_BYTES),
            DirectoryJobClass::Compact
        );
        assert_eq!(
            plan.classify_size(DIRECTORY_COMPACT_FILE_MAX_BYTES + 1),
            DirectoryJobClass::Bulk
        );
    }
}
