fn classify_list_entry_file_type(
    entry_file_type: FileType,
    target_file_type: Option<FileType>,
) -> FileType {
    match entry_file_type {
        FileType::Symlink => match target_file_type {
            Some(FileType::Directory) => FileType::Directory,
            _ => FileType::Symlink,
        },
        other => other,
    }
}

fn file_type_from_attrs(metadata: &FileAttributes) -> FileType {
    if metadata.is_dir() {
        FileType::Directory
    } else if metadata.is_symlink() {
        FileType::Symlink
    } else if metadata.is_regular() {
        FileType::File
    } else {
        FileType::Unknown
    }
}

fn sort_entries(entries: &mut [FileInfo], order: SortOrder) {
    entries.sort_by(|a, b| {
        let a_is_dir = a.file_type == FileType::Directory;
        let b_is_dir = b.file_type == FileType::Directory;
        if a_is_dir != b_is_dir {
            return b_is_dir.cmp(&a_is_dir);
        }
        match order {
            SortOrder::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortOrder::NameDesc => b.name.to_lowercase().cmp(&a.name.to_lowercase()),
            SortOrder::Size => a.size.cmp(&b.size),
            SortOrder::SizeDesc => b.size.cmp(&a.size),
            SortOrder::Modified => a.modified.cmp(&b.modified),
            SortOrder::ModifiedDesc => b.modified.cmp(&a.modified),
            SortOrder::Type => a.name.cmp(&b.name),
            SortOrder::TypeDesc => b.name.cmp(&a.name),
        }
    });
}

fn swap_path(canonical_path: &str) -> String {
    if let Some(slash_pos) = canonical_path.rfind('/') {
        let dir = &canonical_path[..=slash_pos];
        let name = &canonical_path[slash_pos + 1..];
        format!("{dir}.{name}.oxswp")
    } else {
        format!(".{canonical_path}.oxswp")
    }
}

async fn throttle_transfer(
    transferred: u64,
    started: Instant,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
) -> std::time::Duration {
    let Some(manager) = transfer_manager else {
        return std::time::Duration::ZERO;
    };
    let limit = manager.speed_limit_bps();
    if limit == 0 {
        return std::time::Duration::ZERO;
    }
    let elapsed = started.elapsed().as_secs_f64();
    let expected = transferred as f64 / limit as f64;
    if expected > elapsed {
        let sleep = std::time::Duration::from_secs_f64(expected - elapsed);
        tokio::time::sleep(sleep).await;
        return sleep;
    }
    std::time::Duration::ZERO
}

async fn check_transfer_control(
    transfer_manager: &Option<Arc<SftpTransferManager>>,
    transfer_id: &str,
) -> Result<(), SftpError> {
    if let Some(manager) = transfer_manager {
        manager.check_control(transfer_id).await?;
    }
    Ok(())
}

async fn send_transfer_progress(
    progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
    transfer_id: &str,
    remote_path: &str,
    local_path: &str,
    direction: TransferDirection,
    total_bytes: u64,
    transferred_bytes: u64,
    started: Instant,
    state: TransferState,
    error: Option<String>,
) {
    let Some(tx) = progress_tx else {
        return;
    };
    let elapsed = started.elapsed().as_secs_f64();
    let speed = if elapsed > 0.0 {
        (transferred_bytes as f64 / elapsed) as u64
    } else {
        0
    };
    let eta_seconds = if speed > 0 && total_bytes > transferred_bytes {
        Some(((total_bytes - transferred_bytes) as f64 / speed as f64) as u64)
    } else {
        None
    };
    let progress = TransferProgress {
        id: transfer_id.to_string(),
        remote_path: remote_path.to_string(),
        local_path: local_path.to_string(),
        direction,
        state,
        total_bytes,
        transferred_bytes,
        speed,
        eta_seconds,
        error,
    };

    if state == TransferState::InProgress {
        // Intermediate progress is lossy by design; the data plane must not wait
        // for a slow UI or persistence consumer while SFTP requests can keep flowing.
        let _ = tx.try_send(progress);
        return;
    }

    let _ = tx.send(progress).await;
}

fn is_missing_file_error_message(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("no such file")
        || lower.contains("not found")
        || lower.contains("does not exist")
}

struct LocalSftpDiagnostics {
    last_log: Instant,
    local_read_count: u64,
    local_read_bytes: u64,
    local_read_total: std::time::Duration,
    local_write_count: u64,
    local_write_bytes: u64,
    local_write_total: std::time::Duration,
    local_seek_count: u64,
    local_seek_total: std::time::Duration,
    throttle_sleep_count: u64,
    throttle_sleep_total: std::time::Duration,
}

impl LocalSftpDiagnostics {
    const LOG_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3);

    fn new() -> Self {
        Self {
            last_log: Instant::now(),
            local_read_count: 0,
            local_read_bytes: 0,
            local_read_total: std::time::Duration::ZERO,
            local_write_count: 0,
            local_write_bytes: 0,
            local_write_total: std::time::Duration::ZERO,
            local_seek_count: 0,
            local_seek_total: std::time::Duration::ZERO,
            throttle_sleep_count: 0,
            throttle_sleep_total: std::time::Duration::ZERO,
        }
    }

    fn record_local_read(&mut self, bytes: usize, elapsed: std::time::Duration) {
        self.local_read_count = self.local_read_count.saturating_add(1);
        self.local_read_bytes = self.local_read_bytes.saturating_add(bytes as u64);
        self.local_read_total += elapsed;
    }

    fn record_local_write(&mut self, bytes: usize, elapsed: std::time::Duration) {
        self.local_write_count = self.local_write_count.saturating_add(1);
        self.local_write_bytes = self.local_write_bytes.saturating_add(bytes as u64);
        self.local_write_total += elapsed;
    }

    fn record_local_seek(&mut self, elapsed: std::time::Duration) {
        self.local_seek_count = self.local_seek_count.saturating_add(1);
        self.local_seek_total += elapsed;
    }

    fn record_throttle_sleep(&mut self, elapsed: std::time::Duration) {
        if elapsed.is_zero() {
            return;
        }
        self.throttle_sleep_count = self.throttle_sleep_count.saturating_add(1);
        self.throttle_sleep_total += elapsed;
    }

    fn should_log(&mut self) -> bool {
        if !sftp_local_diagnostics_enabled() || self.last_log.elapsed() < Self::LOG_INTERVAL {
            return false;
        }
        self.last_log = Instant::now();
        true
    }

    fn local_read_avg_ms(&self) -> u64 {
        avg_duration_ms(self.local_read_total, self.local_read_count)
    }

    fn local_write_avg_ms(&self) -> u64 {
        avg_duration_ms(self.local_write_total, self.local_write_count)
    }

    fn local_seek_avg_ms(&self) -> u64 {
        avg_duration_ms(self.local_seek_total, self.local_seek_count)
    }

    fn throttle_sleep_ms(&self) -> u64 {
        duration_ms(self.throttle_sleep_total)
    }
}

fn sftp_local_diagnostics_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("OXIDETERM_SFTP_LOCAL_DIAGNOSTICS")
            .map(|value| {
                let normalized = value.trim().to_ascii_lowercase();
                matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
            })
            .unwrap_or(false)
    })
}

fn emit_local_sftp_diagnostics(line: String) {
    // This opt-in diagnostic path writes only aggregate counters to the local
    // stderr stream, so native dev runs do not depend on a tracing subscriber.
    eprintln!("{line}");
}

fn duration_ms(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn avg_duration_ms(duration: std::time::Duration, count: u64) -> u64 {
    if count == 0 {
        return 0;
    }
    duration_ms(duration / count as u32)
}

fn shrink_reason_name(reason: Option<russh_sftp::client::fs::SftpWindowShrinkReason>) -> &'static str {
    reason.map(|reason| reason.as_str()).unwrap_or("none")
}

fn format_download_diagnostics(
    transferred: u64,
    total: u64,
    window: PipelinedDownloaderSnapshot,
    local: &LocalSftpDiagnostics,
) -> String {
    format!(
        concat!(
            "sftp_local_diagnostics direction=download transferred_bytes={} total_bytes={} ",
            "target_requests={} pending_requests={} target_inflight_bytes={} inflight_bytes={} ",
            "target_chunk_len={} cap_requests={} cap_inflight_bytes={} cap_chunk_len={} ",
            "window_bps={} rtt_avg_ms={} rtt_min_ms={} rtt_max_ms={} rtt_p50_ms={} rtt_p95_ms={} ",
            "growths={} shrinks={} shrink_reason={} short_reads={} status_errors={} ",
            "protocol_errors={} channel_closed={} zero_reads={} ready_chunks={} out_of_order={} ",
            "largest_reorder_gap={} discarded_speculative={} discarded_ready={} restarts={} ",
            "local_write_count={} local_write_bytes={} local_write_avg_ms={} ",
            "local_seek_count={} local_seek_avg_ms={} throttle_sleep_ms={}"
        ),
        transferred,
        total,
        window.window.target_requests,
        window.pending_requests,
        window.window.target_inflight_bytes,
        window.inflight_bytes,
        window.window.target_chunk_len,
        window.window.cap_requests,
        window.window.cap_inflight_bytes,
        window.window.cap_chunk_len,
        window.window.bytes_per_sec,
        window.window.rtt_avg_ms,
        window.window.rtt_min_ms,
        window.window.rtt_max_ms,
        window.window.rtt_p50_ms,
        window.window.rtt_p95_ms,
        window.window.window_growth_count,
        window.window.window_shrink_count,
        shrink_reason_name(window.window.last_shrink_reason),
        window.window.short_read_count,
        window.window.status_error_count,
        window.window.protocol_error_count,
        window.window.channel_closed_count,
        window.window.zero_read_count,
        window.ready_chunks,
        window.out_of_order_completed,
        window.largest_reorder_gap,
        window.discarded_speculative_requests,
        window.discarded_ready_chunks,
        window.restart_from_offset_count,
        local.local_write_count,
        local.local_write_bytes,
        local.local_write_avg_ms(),
        local.local_seek_count,
        local.local_seek_avg_ms(),
        local.throttle_sleep_ms(),
    )
}

fn format_upload_diagnostics(
    transferred: u64,
    total: u64,
    window: PipelinedUploaderSnapshot,
    local: &LocalSftpDiagnostics,
) -> String {
    format!(
        concat!(
            "sftp_local_diagnostics direction=upload transferred_bytes={} total_bytes={} ",
            "target_requests={} pending_write_acks={} target_inflight_bytes={} inflight_bytes={} ",
            "target_chunk_len={} cap_requests={} cap_inflight_bytes={} cap_chunk_len={} ",
            "window_bps={} rtt_avg_ms={} rtt_min_ms={} rtt_max_ms={} rtt_p50_ms={} rtt_p95_ms={} ",
            "growths={} shrinks={} shrink_reason={} status_errors={} protocol_errors={} ",
            "channel_closed={} scheduled_bytes={} capacity_wait_count={} capacity_wait_avg_ms={} ",
            "write_status_ok={} write_status_error={} local_read_count={} local_read_bytes={} ",
            "local_read_avg_ms={} throttle_sleep_ms={}"
        ),
        transferred,
        total,
        window.window.target_requests,
        window.pending_write_acks,
        window.window.target_inflight_bytes,
        window.inflight_bytes,
        window.window.target_chunk_len,
        window.window.cap_requests,
        window.window.cap_inflight_bytes,
        window.window.cap_chunk_len,
        window.window.bytes_per_sec,
        window.window.rtt_avg_ms,
        window.window.rtt_min_ms,
        window.window.rtt_max_ms,
        window.window.rtt_p50_ms,
        window.window.rtt_p95_ms,
        window.window.window_growth_count,
        window.window.window_shrink_count,
        shrink_reason_name(window.window.last_shrink_reason),
        window.window.status_error_count,
        window.window.protocol_error_count,
        window.window.channel_closed_count,
        window.scheduled_bytes,
        window.capacity_wait_count,
        window.capacity_wait_avg_ms,
        window.write_status_ok_count,
        window.write_status_error_count,
        local.local_read_count,
        local.local_read_bytes,
        local.local_read_avg_ms(),
        local.throttle_sleep_ms(),
    )
}

#[cfg(test)]
mod local_diagnostics_tests {
    use super::*;
    use russh_sftp::client::fs::{
        PipelinedDownloaderSnapshot, SftpWindowShrinkReason, SftpWindowSnapshot,
    };

    fn window_snapshot() -> SftpWindowSnapshot {
        SftpWindowSnapshot {
            target_requests: 8,
            target_inflight_bytes: 1_048_576,
            target_chunk_len: 131_072,
            cap_requests: 64,
            cap_inflight_bytes: 8_388_608,
            cap_chunk_len: 2_097_152,
            completed_requests: 12,
            completed_bytes: 1_572_864,
            bytes_per_sec: 524_288,
            rtt_avg_ms: 40,
            rtt_min_ms: 20,
            rtt_max_ms: 90,
            rtt_p50_ms: 35,
            rtt_p95_ms: 85,
            window_growth_count: 2,
            window_shrink_count: 1,
            short_read_count: 1,
            status_error_count: 0,
            protocol_error_count: 0,
            channel_closed_count: 0,
            zero_read_count: 0,
            last_shrink_reason: Some(SftpWindowShrinkReason::ShortRead),
        }
    }

    #[test]
    fn local_download_diagnostics_format_contains_no_identity_fields() {
        let mut local = LocalSftpDiagnostics::new();
        local.record_local_write(4096, std::time::Duration::from_millis(2));
        let output = format_download_diagnostics(
            4096,
            8192,
            PipelinedDownloaderSnapshot {
                window: window_snapshot(),
                pending_requests: 2,
                inflight_bytes: 262_144,
                ready_chunks: 1,
                discarded_speculative_requests: 3,
                discarded_ready_chunks: 0,
                out_of_order_completed: 1,
                largest_reorder_gap: 131_072,
                restart_from_offset_count: 1,
            },
            &local,
        );

        assert!(output.contains("direction=download"));
        assert!(output.contains("shrink_reason=short_read"));
        for forbidden in [
            "host",
            "username",
            "user=",
            "path",
            "filename",
            "connection_id",
            "node_id",
            "error_message",
        ] {
            assert!(!output.contains(forbidden), "{forbidden} leaked in {output}");
        }
    }
}
