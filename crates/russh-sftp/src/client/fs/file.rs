use bytes::Bytes;
use std::{
    collections::{BTreeMap, VecDeque},
    future::{self, Future},
    io::{self, SeekFrom},
    mem,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf},
    sync::oneshot,
};

use super::Metadata;
use crate::{
    client::{
        error::Error,
        rawsession::SftpResult,
        session::Features,
        transport::{PendingRequest, TryQueueError},
        RawSftpSession,
    },
    protocol::{Packet, StatusCode},
};

type StateFn<T> = Option<Pin<Box<dyn Future<Output = io::Result<T>> + Send + Sync + 'static>>>;

// read packet overhead: type(1) + id(4) + data_len(4)
const READ_OVERHEAD_LENGTH: u32 = 9;
// write packet overhead excluding handle: type(1) + id(4) + handle_len(4) + offset(8) + data_len(4)
const WRITE_OVERHEAD_LENGTH: u32 = 21;

struct FileState {
    f_read: StateFn<Option<Bytes>>,
    f_seek: StateFn<u64>,
    f_flush: StateFn<()>,
    f_shutdown: StateFn<()>,
    write_acks: VecDeque<PendingRequest>,
}

struct PendingRead {
    offset: u64,
    requested_len: usize,
    sent_at: Instant,
    rx: PendingRequest,
}

struct CompletedRead {
    offset: u64,
    requested_len: usize,
    sent_at: Instant,
    result: SftpResult<Packet>,
}

struct PendingWrite {
    requested_len: usize,
    sent_at: Instant,
    rx: PendingRequest,
}

const WINDOW_TUNER_ADJUST_INTERVAL: Duration = Duration::from_millis(750);
const WINDOW_TUNER_MIN_CHUNK: usize = 64 * 1024;
const WINDOW_TUNER_START_CHUNK: usize = 128 * 1024;
const WINDOW_TUNER_START_REQUESTS: usize = 8;
const WINDOW_TUNER_FAST_RAMP_INTERVALS: usize = 4;
const WINDOW_TUNER_RTT_SAMPLE_CAP: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SftpWindowShrinkReason {
    ShortRead,
    StatusError,
    ProtocolError,
    ChannelClosed,
}

impl SftpWindowShrinkReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ShortRead => "short_read",
            Self::StatusError => "status_error",
            Self::ProtocolError => "protocol_error",
            Self::ChannelClosed => "channel_closed",
        }
    }
}

#[derive(Clone, Copy)]
pub struct SftpWindowSnapshot {
    pub target_requests: usize,
    pub target_inflight_bytes: usize,
    pub target_chunk_len: usize,
    pub cap_requests: usize,
    pub cap_inflight_bytes: usize,
    pub cap_chunk_len: usize,
    pub completed_requests: u64,
    pub completed_bytes: u64,
    pub bytes_per_sec: u64,
    pub rtt_avg_ms: u64,
    pub rtt_min_ms: u64,
    pub rtt_max_ms: u64,
    pub rtt_p50_ms: u64,
    pub rtt_p95_ms: u64,
    pub window_growth_count: u64,
    pub window_shrink_count: u64,
    pub short_read_count: u64,
    pub status_error_count: u64,
    pub protocol_error_count: u64,
    pub channel_closed_count: u64,
    pub zero_read_count: u64,
    pub last_shrink_reason: Option<SftpWindowShrinkReason>,
}

#[derive(Clone, Copy)]
pub struct PipelinedDownloaderSnapshot {
    pub window: SftpWindowSnapshot,
    pub pending_requests: usize,
    pub inflight_bytes: usize,
    pub ready_chunks: usize,
    pub discarded_speculative_requests: u64,
    pub discarded_ready_chunks: u64,
    pub out_of_order_completed: u64,
    pub largest_reorder_gap: u64,
    pub restart_from_offset_count: u64,
}

#[derive(Clone, Copy)]
pub struct PipelinedUploaderSnapshot {
    pub window: SftpWindowSnapshot,
    pub pending_write_acks: usize,
    pub inflight_bytes: usize,
    pub scheduled_bytes: u64,
    pub capacity_wait_count: u64,
    pub capacity_wait_avg_ms: u64,
    pub write_status_ok_count: u64,
    pub write_status_error_count: u64,
}

struct SftpWindowTuner {
    max_requests: usize,
    max_inflight_bytes: usize,
    max_chunk_len: usize,
    target_requests: usize,
    target_inflight_bytes: usize,
    target_chunk_len: usize,
    min_requests: usize,
    min_inflight_bytes: usize,
    min_chunk_len: usize,
    interval_started: Instant,
    completed_requests: usize,
    completed_bytes: usize,
    rtt_total: Duration,
    congestion_events: usize,
    started_at: Instant,
    total_completed_requests: u64,
    total_completed_bytes: u64,
    total_rtt: Duration,
    rtt_min_ms: Option<u64>,
    rtt_max_ms: u64,
    rtt_samples_ms: VecDeque<u64>,
    last_bytes_per_sec: u64,
    window_growth_count: u64,
    window_shrink_count: u64,
    short_read_count: u64,
    status_error_count: u64,
    protocol_error_count: u64,
    channel_closed_count: u64,
    zero_read_count: u64,
    last_shrink_reason: Option<SftpWindowShrinkReason>,
    clean_growth_intervals: usize,
}

impl SftpWindowTuner {
    fn new(max_requests: usize, max_inflight_bytes: usize, max_chunk_len: usize) -> Self {
        let max_requests = max_requests.max(1);
        let max_chunk_len = max_chunk_len.max(1);
        let max_inflight_bytes = max_inflight_bytes.max(max_chunk_len);
        let min_requests = max_requests.min(4).max(1);
        let min_chunk_len = max_chunk_len.min(WINDOW_TUNER_MIN_CHUNK).max(1);
        let min_inflight_bytes = max_inflight_bytes
            .min(min_chunk_len.saturating_mul(min_requests))
            .max(min_chunk_len);
        let target_requests = max_requests
            .min(WINDOW_TUNER_START_REQUESTS)
            .max(min_requests);
        let target_chunk_len = max_chunk_len
            .min(WINDOW_TUNER_START_CHUNK)
            .max(min_chunk_len);
        let target_inflight_bytes = max_inflight_bytes
            .min(target_chunk_len.saturating_mul(target_requests))
            .max(min_inflight_bytes);

        Self {
            max_requests,
            max_inflight_bytes,
            max_chunk_len,
            target_requests,
            target_inflight_bytes,
            target_chunk_len,
            min_requests,
            min_inflight_bytes,
            min_chunk_len,
            interval_started: Instant::now(),
            completed_requests: 0,
            completed_bytes: 0,
            rtt_total: Duration::ZERO,
            congestion_events: 0,
            started_at: Instant::now(),
            total_completed_requests: 0,
            total_completed_bytes: 0,
            total_rtt: Duration::ZERO,
            rtt_min_ms: None,
            rtt_max_ms: 0,
            rtt_samples_ms: VecDeque::with_capacity(WINDOW_TUNER_RTT_SAMPLE_CAP),
            last_bytes_per_sec: 0,
            window_growth_count: 0,
            window_shrink_count: 0,
            short_read_count: 0,
            status_error_count: 0,
            protocol_error_count: 0,
            channel_closed_count: 0,
            zero_read_count: 0,
            last_shrink_reason: None,
            clean_growth_intervals: 0,
        }
    }

    fn target_requests(&self) -> usize {
        self.target_requests
    }

    fn target_inflight_bytes(&self) -> usize {
        self.target_inflight_bytes
    }

    fn target_chunk_len(&self) -> usize {
        self.target_chunk_len
    }

    fn record_success(&mut self, bytes: usize, rtt: Duration) {
        self.record_completion(bytes, rtt);
        self.completed_requests = self.completed_requests.saturating_add(1);
        self.completed_bytes = self.completed_bytes.saturating_add(bytes);
        self.rtt_total += rtt;

        let elapsed = self.interval_started.elapsed();
        if elapsed >= WINDOW_TUNER_ADJUST_INTERVAL {
            self.adjust_after_interval(elapsed);
        }
    }

    fn record_short_read(&mut self, bytes: usize, rtt: Duration) {
        // SFTP servers may legally return less data than requested. Count the
        // event without treating it as congestion or shrinking the window.
        self.short_read_count = self.short_read_count.saturating_add(1);
        self.record_success(bytes, rtt);
    }

    fn record_zero_read(&mut self) {
        self.zero_read_count = self.zero_read_count.saturating_add(1);
    }

    fn record_error(&mut self, reason: SftpWindowShrinkReason) {
        self.record_congestion(reason);
    }

    fn record_congestion(&mut self, reason: SftpWindowShrinkReason) {
        self.record_shrink_reason(reason);
        self.congestion_events = self.congestion_events.saturating_add(1);
        self.target_requests = (self.target_requests / 2).max(self.min_requests);
        self.target_inflight_bytes = (self.target_inflight_bytes / 2).max(self.min_inflight_bytes);
        self.target_chunk_len = (self.target_chunk_len / 2).max(self.min_chunk_len);
        self.clean_growth_intervals = 0;
        self.window_shrink_count = self.window_shrink_count.saturating_add(1);
        self.reset_interval();
        self.clamp_targets();
    }

    fn record_completion(&mut self, bytes: usize, rtt: Duration) {
        let rtt_ms = duration_millis(rtt);
        self.total_completed_requests = self.total_completed_requests.saturating_add(1);
        self.total_completed_bytes = self.total_completed_bytes.saturating_add(bytes as u64);
        self.total_rtt += rtt;
        self.rtt_min_ms = Some(self.rtt_min_ms.map_or(rtt_ms, |min| min.min(rtt_ms)));
        self.rtt_max_ms = self.rtt_max_ms.max(rtt_ms);
        if self.rtt_samples_ms.len() >= WINDOW_TUNER_RTT_SAMPLE_CAP {
            self.rtt_samples_ms.pop_front();
        }
        self.rtt_samples_ms.push_back(rtt_ms);
    }

    fn record_shrink_reason(&mut self, reason: SftpWindowShrinkReason) {
        match reason {
            SftpWindowShrinkReason::ShortRead => {
                self.short_read_count = self.short_read_count.saturating_add(1);
            }
            SftpWindowShrinkReason::StatusError => {
                self.status_error_count = self.status_error_count.saturating_add(1);
            }
            SftpWindowShrinkReason::ProtocolError => {
                self.protocol_error_count = self.protocol_error_count.saturating_add(1);
            }
            SftpWindowShrinkReason::ChannelClosed => {
                self.channel_closed_count = self.channel_closed_count.saturating_add(1);
            }
        }
        self.last_shrink_reason = Some(reason);
    }

    fn adjust_after_interval(&mut self, elapsed: Duration) {
        if self.completed_requests == 0 {
            self.reset_interval();
            return;
        }

        if self.congestion_events == 0 {
            let avg_rtt = self.rtt_total / self.completed_requests as u32;
            let bytes_per_sec =
                (self.completed_bytes as f64 / elapsed.as_secs_f64().max(0.001)) as usize;
            self.last_bytes_per_sec = bytes_per_sec as u64;
            let desired_chunk = Self::chunk_for_throughput(bytes_per_sec)
                .clamp(self.min_chunk_len, self.max_chunk_len);
            self.target_chunk_len = if desired_chunk >= self.target_chunk_len {
                desired_chunk
            } else {
                (self.target_chunk_len / 2).max(desired_chunk)
            };

            // Unknown servers start conservatively, then clean early intervals
            // ramp faster so high-RTT links can fill the pipe without waiting
            // through many linear growth windows.
            let request_step = self.request_growth_step(avg_rtt);
            self.target_requests = self.next_request_target(request_step);
            let inflight_step = self.target_chunk_len.saturating_mul(request_step);
            self.target_inflight_bytes = self
                .target_inflight_bytes
                .saturating_add(inflight_step)
                .min(self.max_inflight_bytes);
            self.clean_growth_intervals = self.clean_growth_intervals.saturating_add(1);
            self.window_growth_count = self.window_growth_count.saturating_add(1);
        }

        self.reset_interval();
        self.clamp_targets();
    }

    fn request_growth_step(&self, avg_rtt: Duration) -> usize {
        let base_step = if avg_rtt >= Duration::from_millis(150) {
            8
        } else {
            4
        };

        if self.clean_growth_intervals < WINDOW_TUNER_FAST_RAMP_INTERVALS {
            base_step * 2
        } else {
            base_step
        }
    }

    fn next_request_target(&self, request_step: usize) -> usize {
        let linear_target = self.target_requests.saturating_add(request_step);
        if self.clean_growth_intervals < WINDOW_TUNER_FAST_RAMP_INTERVALS {
            linear_target.max(self.target_requests.saturating_mul(2))
        } else {
            linear_target
        }
        .min(self.max_requests)
    }

    fn chunk_for_throughput(bytes_per_sec: usize) -> usize {
        match bytes_per_sec {
            0..=262_144 => WINDOW_TUNER_MIN_CHUNK,
            262_145..=1_048_576 => 128 * 1024,
            1_048_577..=10_485_760 => 256 * 1024,
            10_485_761..=52_428_800 => 1_048_576,
            _ => 2 * 1024 * 1024,
        }
    }

    fn reset_interval(&mut self) {
        self.interval_started = Instant::now();
        self.completed_requests = 0;
        self.completed_bytes = 0;
        self.rtt_total = Duration::ZERO;
        self.congestion_events = 0;
    }

    fn clamp_targets(&mut self) {
        self.target_requests = self
            .target_requests
            .clamp(self.min_requests, self.max_requests);
        self.target_chunk_len = self
            .target_chunk_len
            .clamp(self.min_chunk_len, self.max_chunk_len);
        self.target_inflight_bytes = self
            .target_inflight_bytes
            .clamp(self.min_inflight_bytes, self.max_inflight_bytes)
            .max(self.target_chunk_len);
    }

    fn snapshot(&self) -> SftpWindowSnapshot {
        let avg_rtt_ms = if self.total_completed_requests > 0 {
            duration_millis(self.total_rtt / self.total_completed_requests as u32)
        } else {
            0
        };
        let bytes_per_sec = if self.last_bytes_per_sec > 0 {
            self.last_bytes_per_sec
        } else {
            let elapsed = self.started_at.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                (self.total_completed_bytes as f64 / elapsed) as u64
            } else {
                0
            }
        };

        SftpWindowSnapshot {
            target_requests: self.target_requests,
            target_inflight_bytes: self.target_inflight_bytes,
            target_chunk_len: self.target_chunk_len,
            cap_requests: self.max_requests,
            cap_inflight_bytes: self.max_inflight_bytes,
            cap_chunk_len: self.max_chunk_len,
            completed_requests: self.total_completed_requests,
            completed_bytes: self.total_completed_bytes,
            bytes_per_sec,
            rtt_avg_ms: avg_rtt_ms,
            rtt_min_ms: self.rtt_min_ms.unwrap_or_default(),
            rtt_max_ms: self.rtt_max_ms,
            rtt_p50_ms: percentile(&self.rtt_samples_ms, 50),
            rtt_p95_ms: percentile(&self.rtt_samples_ms, 95),
            window_growth_count: self.window_growth_count,
            window_shrink_count: self.window_shrink_count,
            short_read_count: self.short_read_count,
            status_error_count: self.status_error_count,
            protocol_error_count: self.protocol_error_count,
            channel_closed_count: self.channel_closed_count,
            zero_read_count: self.zero_read_count,
            last_shrink_reason: self.last_shrink_reason,
        }
    }
}

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn percentile(samples: &VecDeque<u64>, percentile: u8) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let mut values = samples.iter().copied().collect::<Vec<_>>();
    values.sort_unstable();
    let numerator = (values.len().saturating_sub(1)).saturating_mul(percentile as usize);
    let index = (numerator + 50) / 100;
    values[index.min(values.len() - 1)]
}

/// A chunk returned by a pipelined SFTP file read.
pub struct PipelinedReadChunk {
    pub offset: u64,
    pub data: Bytes,
}

/// Download-only ownership of a remote file handle.
pub struct FileDownloadParts {
    session: Arc<RawSftpSession>,
    handle: String,
    max_read_len: usize,
    closed: bool,
}

/// Upload-only ownership of a remote file handle.
pub struct FileUploadParts {
    session: Arc<RawSftpSession>,
    handle: String,
    max_write_len: usize,
    fsync: bool,
    closed: bool,
}

/// Sequentially emits file chunks while keeping raw SFTP reads in flight.
pub struct PipelinedFileDownloader {
    session: Arc<RawSftpSession>,
    handle: String,
    pending: VecDeque<PendingRead>,
    repair_reads: BTreeMap<u64, usize>,
    ready_chunks: BTreeMap<u64, Bytes>,
    next_request_offset: u64,
    next_write_offset: u64,
    end_offset: Option<u64>,
    max_read_len: usize,
    inflight_bytes: usize,
    tuner: SftpWindowTuner,
    discarded_speculative_requests: u64,
    discarded_ready_chunks: u64,
    out_of_order_completed: u64,
    largest_reorder_gap: u64,
    restart_from_offset_count: u64,
    scheduling_error: Option<Error>,
    finished: bool,
    closed: bool,
}

/// Sequentially writes file chunks while keeping raw SFTP writes in flight.
pub struct PipelinedFileUploader {
    session: Arc<RawSftpSession>,
    handle: String,
    pending: VecDeque<PendingWrite>,
    next_write_offset: u64,
    max_write_len: usize,
    inflight_bytes: usize,
    tuner: SftpWindowTuner,
    scheduled_bytes: u64,
    capacity_wait_count: u64,
    capacity_wait_total: Duration,
    write_status_ok_count: u64,
    write_status_error_count: u64,
    fsync: bool,
    closed: bool,
}

/// Provides high-level methods for interaction with a remote file.
///
/// In order to properly close the handle, [`shutdown`] on a file should be called.
/// Also implement [`AsyncSeek`] and other async i/o implementations.
///
/// # Weakness
/// Using [`SeekFrom::End`] is costly and time-consuming because we need to
/// request the actual file size from the remote server.
pub struct File {
    session: Arc<RawSftpSession>,
    handle: String,
    state: FileState,
    pos: u64,
    closed: bool,
    features: Features,
}

impl File {
    pub(crate) fn new(session: Arc<RawSftpSession>, handle: String, features: Features) -> Self {
        Self {
            session,
            handle,
            state: FileState {
                f_read: None,
                f_seek: None,
                f_flush: None,
                f_shutdown: None,
                write_acks: VecDeque::with_capacity(features.max_concurrent_writes),
            },
            pos: 0,
            closed: false,
            features,
        }
    }

    /// Queries metadata about the remote file.
    pub async fn metadata(&self) -> SftpResult<Metadata> {
        Ok(self.session.fstat(self.handle.as_str()).await?.attrs)
    }

    /// Sets metadata for a remote file.
    pub async fn set_metadata(&self, metadata: Metadata) -> SftpResult<()> {
        self.session
            .fsetstat(self.handle.as_str(), metadata)
            .await
            .map(|_| ())
    }

    /// Attempts to sync all data.
    ///
    /// If the server does not support `fsync@openssh.com` sending the request will
    /// be omitted, but will still pseudo-successfully
    pub async fn sync_all(&self) -> SftpResult<()> {
        if !self.features.fsync {
            return Ok(());
        }

        self.session.fsync(self.handle.as_str()).await.map(|_| ())
    }

    /// Converts this file into a sequential pipelined reader starting at `offset`.
    ///
    /// The regular `AsyncRead` implementation intentionally remains single-request
    /// to preserve stream semantics. This reader is for bulk sequential downloads.
    pub fn into_pipelined_reader(
        self,
        offset: u64,
        max_chunk_len: usize,
        max_concurrent_reads: usize,
    ) -> PipelinedFileDownloader {
        self.into_pipelined_reader_for_range(offset, None, max_chunk_len, max_concurrent_reads)
    }

    /// Converts this file into a pipelined reader bounded by an optional end offset.
    ///
    /// Supplying `end_offset` lets bulk downloads avoid speculative reads beyond
    /// a known remote file size.
    pub fn into_pipelined_reader_for_range(
        self,
        offset: u64,
        end_offset: Option<u64>,
        max_chunk_len: usize,
        max_concurrent_reads: usize,
    ) -> PipelinedFileDownloader {
        let max_read_len = self.max_read_len().min(max_chunk_len).max(1);
        let max_inflight_bytes = max_read_len.saturating_mul(max_concurrent_reads.max(1));
        self.into_pipelined_downloader_for_range(
            offset,
            end_offset,
            max_chunk_len,
            max_concurrent_reads,
            max_inflight_bytes,
        )
    }

    /// Converts this file into an OpenSSH-style raw request downloader.
    pub fn into_pipelined_downloader_for_range(
        self,
        offset: u64,
        end_offset: Option<u64>,
        max_chunk_len: usize,
        max_requests: usize,
        max_inflight_bytes: usize,
    ) -> PipelinedFileDownloader {
        let parts = self.into_download_parts(max_chunk_len);
        PipelinedFileDownloader::new(parts, offset, end_offset, max_requests, max_inflight_bytes)
    }

    /// Gives bulk download code ownership of the remote handle and limits.
    pub fn into_download_parts(mut self, max_chunk_len: usize) -> FileDownloadParts {
        let max_read_len = self.max_read_len().min(max_chunk_len).max(1);
        let parts = FileDownloadParts {
            session: self.session.clone(),
            handle: self.handle.clone(),
            max_read_len,
            closed: false,
        };

        // DownloadParts now owns closing the remote handle.
        self.closed = true;
        parts
    }

    /// Converts this file into a raw request uploader starting at `offset`.
    pub fn into_pipelined_uploader(
        self,
        offset: u64,
        max_chunk_len: usize,
        max_requests: usize,
        max_inflight_bytes: usize,
    ) -> PipelinedFileUploader {
        let parts = self.into_upload_parts(max_chunk_len);
        PipelinedFileUploader::new(parts, offset, max_requests, max_inflight_bytes)
    }

    /// Gives bulk upload code ownership of the remote handle and limits.
    pub fn into_upload_parts(mut self, max_chunk_len: usize) -> FileUploadParts {
        let max_write_len = self.max_write_len().min(max_chunk_len).max(1);
        let parts = FileUploadParts {
            session: self.session.clone(),
            handle: self.handle.clone(),
            max_write_len,
            fsync: self.features.fsync,
            closed: false,
        };

        // UploadParts now owns flushing and closing the remote handle.
        self.closed = true;
        parts
    }

    fn max_read_len(&self) -> usize {
        self.features
            .limits
            .and_then(|l| l.read_len)
            .unwrap_or_else(|| {
                self.features
                    .max_packet_len
                    .saturating_sub(READ_OVERHEAD_LENGTH) as u64
            }) as usize
    }

    fn max_write_len(&self) -> usize {
        self.features
            .limits
            .and_then(|l| l.write_len)
            .unwrap_or_else(|| {
                let overhead = WRITE_OVERHEAD_LENGTH + self.handle.len() as u32;
                self.features.max_packet_len.saturating_sub(overhead) as u64
            }) as usize
    }
}

impl FileDownloadParts {
    pub fn session(&self) -> Arc<RawSftpSession> {
        self.session.clone()
    }

    pub fn handle(&self) -> &str {
        &self.handle
    }

    pub fn max_read_len(&self) -> usize {
        self.max_read_len
    }

    fn into_components(mut self) -> (Arc<RawSftpSession>, String, usize) {
        self.closed = true;
        (
            self.session.clone(),
            mem::take(&mut self.handle),
            self.max_read_len,
        )
    }
}

impl Drop for FileDownloadParts {
    fn drop(&mut self) {
        if self.closed {
            return;
        }

        let _ = self.session.close_nowait(mem::take(&mut self.handle));
    }
}

impl FileUploadParts {
    pub fn session(&self) -> Arc<RawSftpSession> {
        self.session.clone()
    }

    pub fn handle(&self) -> &str {
        &self.handle
    }

    pub fn max_write_len(&self) -> usize {
        self.max_write_len
    }

    fn into_components(mut self) -> (Arc<RawSftpSession>, String, usize, bool) {
        self.closed = true;
        (
            self.session.clone(),
            mem::take(&mut self.handle),
            self.max_write_len,
            self.fsync,
        )
    }
}

impl Drop for FileUploadParts {
    fn drop(&mut self) {
        if self.closed {
            return;
        }

        let _ = self.session.close_nowait(mem::take(&mut self.handle));
    }
}

impl PipelinedFileDownloader {
    pub fn new(
        parts: FileDownloadParts,
        offset: u64,
        end_offset: Option<u64>,
        max_requests: usize,
        max_inflight_bytes: usize,
    ) -> Self {
        let (session, handle, max_read_len) = parts.into_components();
        let max_requests = max_requests.max(1);
        let max_inflight_bytes = max_inflight_bytes.max(max_read_len);
        let tuner = SftpWindowTuner::new(max_requests, max_inflight_bytes, max_read_len);
        Self {
            session,
            handle,
            pending: VecDeque::with_capacity(max_requests),
            repair_reads: BTreeMap::new(),
            ready_chunks: BTreeMap::new(),
            next_request_offset: offset,
            next_write_offset: offset,
            end_offset,
            max_read_len,
            inflight_bytes: 0,
            tuner,
            discarded_speculative_requests: 0,
            discarded_ready_chunks: 0,
            out_of_order_completed: 0,
            largest_reorder_gap: 0,
            restart_from_offset_count: 0,
            scheduling_error: None,
            finished: false,
            closed: false,
        }
    }

    async fn fill_pending(&mut self) {
        if self.finished {
            return;
        }

        while self.pending.len() < self.tuner.target_requests()
            && self.inflight_bytes < self.tuner.target_inflight_bytes()
        {
            let remaining_inflight = self
                .tuner
                .target_inflight_bytes()
                .saturating_sub(self.inflight_bytes);
            let Some((offset, requested_len)) = self.next_read_range(remaining_inflight) else {
                break;
            };
            let rx = match self
                .session
                .read_nowait_raw(&self.handle, offset, requested_len as u32)
                .await
            {
                Ok(rx) => rx,
                Err(error) => {
                    self.tuner
                        .record_error(SftpWindowShrinkReason::ProtocolError);
                    self.scheduling_error = Some(error);
                    self.finished = true;
                    self.discard_pending_after_fault();
                    break;
                }
            };

            self.inflight_bytes = self.inflight_bytes.saturating_add(requested_len);
            self.pending.push_back(PendingRead {
                offset,
                requested_len,
                sent_at: Instant::now(),
                rx,
            });
        }
    }

    fn next_read_range(&mut self, remaining_inflight: usize) -> Option<(u64, usize)> {
        if let Some((&offset, &repair_len)) = self.repair_reads.first_key_value() {
            self.repair_reads.remove(&offset);
            let requested_len = repair_len
                .min(self.max_read_len)
                .min(self.tuner.target_chunk_len())
                .min(remaining_inflight)
                .max(1);
            if requested_len < repair_len {
                self.repair_reads.insert(
                    offset.saturating_add(requested_len as u64),
                    repair_len - requested_len,
                );
            }
            return Some((offset, requested_len));
        }

        if self
            .end_offset
            .is_some_and(|end_offset| self.next_request_offset >= end_offset)
        {
            return None;
        }

        let offset = self.next_request_offset;
        let requested_len = self
            .end_offset
            .map(|end_offset| {
                usize::try_from(end_offset.saturating_sub(offset)).unwrap_or(usize::MAX)
            })
            .map(|remaining| remaining.min(self.max_read_len))
            .unwrap_or(self.max_read_len)
            .min(self.tuner.target_chunk_len())
            .min(remaining_inflight)
            .max(1);
        self.next_request_offset = self
            .next_request_offset
            .saturating_add(requested_len as u64);
        Some((offset, requested_len))
    }

    fn queue_repair_read(&mut self, offset: u64, requested_len: usize) {
        if requested_len == 0 {
            return;
        }

        // Every repair range is the unsatisfied tail of one unique request.
        // Keeping ranges ordered prioritizes the earliest output gap.
        let previous = self.repair_reads.insert(offset, requested_len);
        debug_assert!(previous.is_none(), "repair read ranges must not overlap");
    }

    fn discard_pending(&mut self) {
        for pending in self.pending.drain(..) {
            self.inflight_bytes = self.inflight_bytes.saturating_sub(pending.requested_len);
        }
    }

    fn discard_pending_after_fault(&mut self) {
        self.discarded_speculative_requests = self
            .discarded_speculative_requests
            .saturating_add(self.pending.len() as u64);
        self.discard_pending();
    }

    fn early_eof_error(&mut self, response_offset: u64, expected_end_offset: u64) -> Error {
        self.finished = true;
        self.discard_pending_after_fault();
        Error::UnexpectedBehavior(format!(
            "server returned EOF at offset {response_offset} before expected end offset {expected_end_offset}"
        ))
    }

    fn poll_completed_read(&mut self, cx: &mut Context<'_>) -> Poll<Option<CompletedRead>> {
        let mut ready_index = None;
        let mut ready_result = None;

        for index in 0..self.pending.len() {
            let poll = {
                let pending = &mut self.pending[index];
                Pin::new(&mut pending.rx).poll(cx)
            };

            match poll {
                Poll::Pending => {}
                Poll::Ready(result) => {
                    ready_index = Some(index);
                    ready_result = Some(result);
                    break;
                }
            }
        }

        let Some(index) = ready_index else {
            return if self.pending.is_empty() {
                Poll::Ready(None)
            } else {
                Poll::Pending
            };
        };

        let pending = self
            .pending
            .remove(index)
            .expect("pending read index exists");
        self.inflight_bytes = self.inflight_bytes.saturating_sub(pending.requested_len);
        let result = match ready_result.expect("ready result exists") {
            Ok(result) => result,
            Err(_) => Err(Error::UnexpectedBehavior("read channel closed".into())),
        };

        Poll::Ready(Some(CompletedRead {
            offset: pending.offset,
            requested_len: pending.requested_len,
            sent_at: pending.sent_at,
            result,
        }))
    }

    fn process_completed_read(&mut self, completed: CompletedRead) -> SftpResult<()> {
        let rtt = completed.sent_at.elapsed();
        match completed.result {
            Ok(Packet::Data(data)) => {
                let read_len = data.data.len();
                if completed.offset != self.next_write_offset {
                    self.out_of_order_completed = self.out_of_order_completed.saturating_add(1);
                    let gap = completed.offset.abs_diff(self.next_write_offset);
                    self.largest_reorder_gap = self.largest_reorder_gap.max(gap);
                }

                if read_len == 0 {
                    self.tuner.record_zero_read();
                    if let Some(end_offset) = self
                        .end_offset
                        .filter(|end_offset| completed.offset < *end_offset)
                    {
                        return Err(self.early_eof_error(completed.offset, end_offset));
                    }
                    self.finished = true;
                    self.discard_pending();
                    return Ok(());
                }

                if read_len < completed.requested_len {
                    self.tuner.record_short_read(read_len, rtt);
                    let repair_offset = completed.offset.saturating_add(read_len as u64);
                    self.queue_repair_read(
                        repair_offset,
                        completed.requested_len.saturating_sub(read_len),
                    );
                } else {
                    self.tuner.record_success(read_len, rtt);
                }

                self.ready_chunks.insert(completed.offset, data.data);
                Ok(())
            }
            Ok(Packet::Status(status)) if status.status_code == StatusCode::Eof => {
                if let Some(end_offset) = self
                    .end_offset
                    .filter(|end_offset| completed.offset < *end_offset)
                {
                    return Err(self.early_eof_error(completed.offset, end_offset));
                }
                self.finished = true;
                self.discard_pending();
                Ok(())
            }
            Ok(Packet::Status(status)) => {
                self.discard_pending_after_fault();
                self.tuner.record_error(SftpWindowShrinkReason::StatusError);
                Err(status.into())
            }
            Ok(_) => {
                self.discard_pending_after_fault();
                self.tuner
                    .record_error(SftpWindowShrinkReason::ProtocolError);
                Err(Error::UnexpectedPacket)
            }
            Err(error) => {
                self.discard_pending_after_fault();
                self.tuner
                    .record_error(SftpWindowShrinkReason::ChannelClosed);
                Err(error)
            }
        }
    }

    /// Returns the next chunk in file order.
    ///
    /// Responses may arrive out of order. Chunks are buffered until the next
    /// contiguous file offset is ready for the caller to write.
    pub async fn next_chunk(&mut self) -> SftpResult<Option<PipelinedReadChunk>> {
        loop {
            self.fill_pending().await;

            if let Some(data) = self.ready_chunks.remove(&self.next_write_offset) {
                let offset = self.next_write_offset;
                self.next_write_offset = self.next_write_offset.saturating_add(data.len() as u64);
                return Ok(Some(PipelinedReadChunk { offset, data }));
            }

            if let Some(error) = self.scheduling_error.take() {
                return Err(error);
            }

            if self
                .end_offset
                .is_some_and(|end_offset| self.next_write_offset >= end_offset)
            {
                self.finished = true;
            }

            if self.finished && self.pending.is_empty() {
                return Ok(None);
            }

            let completed = future::poll_fn(|cx| self.poll_completed_read(cx)).await;
            let Some(completed) = completed else {
                return Ok(None);
            };
            self.process_completed_read(completed)?;
        }
    }

    /// Closes the remote file handle after pending reads have been discarded.
    pub async fn shutdown(mut self) -> SftpResult<()> {
        self.discard_pending();
        let result = self.session.close(self.handle.clone()).await.map(|_| ());
        if result.is_ok() {
            self.closed = true;
        }
        result
    }
}

impl Drop for PipelinedFileDownloader {
    fn drop(&mut self) {
        self.discard_pending();
        if self.closed {
            return;
        }

        let _ = self.session.close_nowait(mem::take(&mut self.handle));
    }
}

impl PipelinedFileDownloader {
    /// Returns true when no further chunks can be produced.
    pub fn is_finished(&self) -> bool {
        self.finished && self.pending.is_empty() && self.ready_chunks.is_empty()
    }

    /// Returns the number of raw read requests currently awaiting responses.
    pub fn pending_requests(&self) -> usize {
        self.pending.len()
    }

    /// Returns the currently scheduled but not-yet-received byte count.
    pub fn inflight_bytes(&self) -> usize {
        self.inflight_bytes
    }

    /// Returns the next contiguous offset that will be emitted.
    pub fn next_write_offset(&self) -> u64 {
        self.next_write_offset
    }

    /// Returns the next remote offset that will be requested.
    pub fn next_request_offset(&self) -> u64 {
        self.next_request_offset
    }

    pub fn diagnostic_snapshot(&self) -> PipelinedDownloaderSnapshot {
        PipelinedDownloaderSnapshot {
            window: self.tuner.snapshot(),
            pending_requests: self.pending.len(),
            inflight_bytes: self.inflight_bytes,
            ready_chunks: self.ready_chunks.len(),
            discarded_speculative_requests: self.discarded_speculative_requests,
            discarded_ready_chunks: self.discarded_ready_chunks,
            out_of_order_completed: self.out_of_order_completed,
            largest_reorder_gap: self.largest_reorder_gap,
            restart_from_offset_count: self.restart_from_offset_count,
        }
    }
}

impl PipelinedFileUploader {
    pub fn new(
        parts: FileUploadParts,
        offset: u64,
        max_requests: usize,
        max_inflight_bytes: usize,
    ) -> Self {
        let (session, handle, max_write_len, fsync) = parts.into_components();
        let max_requests = max_requests.max(1);
        let max_inflight_bytes = max_inflight_bytes.max(max_write_len);
        let tuner = SftpWindowTuner::new(max_requests, max_inflight_bytes, max_write_len);
        Self {
            session,
            handle,
            pending: VecDeque::with_capacity(max_requests),
            next_write_offset: offset,
            max_write_len,
            inflight_bytes: 0,
            tuner,
            scheduled_bytes: 0,
            capacity_wait_count: 0,
            capacity_wait_total: Duration::ZERO,
            write_status_ok_count: 0,
            write_status_error_count: 0,
            fsync,
            closed: false,
        }
    }

    fn has_write_capacity(&self) -> bool {
        self.pending.len() < self.tuner.target_requests()
            && self.inflight_bytes < self.tuner.target_inflight_bytes()
    }

    fn poll_write_progress(
        &mut self,
        cx: &mut Context<'_>,
        stop_when_capacity_available: bool,
    ) -> Poll<SftpResult<usize>> {
        let mut completed_bytes = 0usize;

        loop {
            let mut ready_index = None;
            let mut ready_result = None;

            for index in 0..self.pending.len() {
                let poll = {
                    let pending = &mut self.pending[index];
                    Pin::new(&mut pending.rx).poll(cx)
                };

                match poll {
                    Poll::Pending => {}
                    Poll::Ready(result) => {
                        ready_index = Some(index);
                        ready_result = Some(result);
                        break;
                    }
                }
            }

            let Some(index) = ready_index else {
                return if completed_bytes > 0
                    || self.pending.is_empty()
                    || (stop_when_capacity_available && self.has_write_capacity())
                {
                    Poll::Ready(Ok(completed_bytes))
                } else {
                    Poll::Pending
                };
            };

            let pending = self
                .pending
                .remove(index)
                .expect("pending write index exists");
            self.inflight_bytes = self.inflight_bytes.saturating_sub(pending.requested_len);
            let completed = match ready_result.expect("ready result exists") {
                Ok(result) => match check_write_packet(result) {
                    Ok(()) => {
                        self.tuner
                            .record_success(pending.requested_len, pending.sent_at.elapsed());
                        self.write_status_ok_count = self.write_status_ok_count.saturating_add(1);
                        pending.requested_len
                    }
                    Err(error) => {
                        self.write_status_error_count =
                            self.write_status_error_count.saturating_add(1);
                        self.tuner.record_error(SftpWindowShrinkReason::StatusError);
                        return Poll::Ready(Err(error));
                    }
                },
                Err(_) => {
                    self.write_status_error_count = self.write_status_error_count.saturating_add(1);
                    self.tuner
                        .record_error(SftpWindowShrinkReason::ChannelClosed);
                    return Poll::Ready(Err(Error::UnexpectedBehavior(
                        "write channel closed".into(),
                    )));
                }
            };
            completed_bytes = completed_bytes.saturating_add(completed);

            if self.pending.is_empty() {
                return Poll::Ready(Ok(completed_bytes));
            }
        }
    }

    async fn wait_for_capacity(&mut self) -> SftpResult<()> {
        let started = (!self.has_write_capacity()).then(Instant::now);
        while !self.has_write_capacity() {
            let _completed = future::poll_fn(|cx| self.poll_write_progress(cx, true)).await?;
        }
        if let Some(started) = started {
            self.capacity_wait_count = self.capacity_wait_count.saturating_add(1);
            self.capacity_wait_total += started.elapsed();
        }
        Ok(())
    }

    async fn drain_pending(&mut self) -> SftpResult<()> {
        while !self.pending.is_empty() {
            let _completed = future::poll_fn(|cx| self.poll_write_progress(cx, false)).await?;
        }
        Ok(())
    }

    /// Schedules `data` at the current remote offset and returns when queued.
    ///
    /// The returned byte count is scheduled, not necessarily acknowledged; call
    /// `shutdown` to drain every acknowledgement before treating the upload as
    /// durable.
    pub async fn write_all_chunk(&mut self, mut data: &[u8]) -> SftpResult<usize> {
        let mut scheduled = 0usize;
        while !data.is_empty() {
            self.wait_for_capacity().await?;

            let remaining_inflight = self
                .tuner
                .target_inflight_bytes()
                .saturating_sub(self.inflight_bytes);
            let len = data
                .len()
                .min(self.max_write_len)
                .min(self.tuner.target_chunk_len())
                .min(remaining_inflight);
            if len == 0 {
                self.wait_for_capacity().await?;
                continue;
            }

            let offset = self.next_write_offset;
            let rx = self
                .session
                .write_nowait_raw(&self.handle, offset, &data[..len])
                .await?;
            self.pending.push_back(PendingWrite {
                requested_len: len,
                sent_at: Instant::now(),
                rx,
            });
            self.inflight_bytes = self.inflight_bytes.saturating_add(len);
            self.next_write_offset = self.next_write_offset.saturating_add(len as u64);
            scheduled = scheduled.saturating_add(len);
            self.scheduled_bytes = self.scheduled_bytes.saturating_add(len as u64);
            data = &data[len..];
        }

        Ok(scheduled)
    }

    /// Drains pending writes, fsyncs when supported, and closes the remote file.
    pub async fn shutdown(mut self) -> SftpResult<()> {
        self.drain_pending().await?;
        if self.fsync {
            self.session.fsync(self.handle.clone()).await?;
        }
        let result = self.session.close(self.handle.clone()).await.map(|_| ());
        if result.is_ok() {
            self.closed = true;
        }
        result
    }

    pub fn pending_requests(&self) -> usize {
        self.pending.len()
    }

    pub fn inflight_bytes(&self) -> usize {
        self.inflight_bytes
    }

    pub fn next_write_offset(&self) -> u64 {
        self.next_write_offset
    }

    pub fn target_chunk_len(&self) -> usize {
        self.tuner.target_chunk_len()
    }

    pub fn diagnostic_snapshot(&self) -> PipelinedUploaderSnapshot {
        let capacity_wait_avg_ms = if self.capacity_wait_count > 0 {
            duration_millis(self.capacity_wait_total / self.capacity_wait_count as u32)
        } else {
            0
        };

        PipelinedUploaderSnapshot {
            window: self.tuner.snapshot(),
            pending_write_acks: self.pending.len(),
            inflight_bytes: self.inflight_bytes,
            scheduled_bytes: self.scheduled_bytes,
            capacity_wait_count: self.capacity_wait_count,
            capacity_wait_avg_ms,
            write_status_ok_count: self.write_status_ok_count,
            write_status_error_count: self.write_status_error_count,
        }
    }
}

impl Drop for PipelinedFileUploader {
    fn drop(&mut self) {
        self.pending.clear();
        self.inflight_bytes = 0;
        if self.closed {
            return;
        }

        let _ = self.session.close_nowait(mem::take(&mut self.handle));
    }
}

fn check_write_result(
    result: Result<SftpResult<Packet>, oneshot::error::RecvError>,
) -> io::Result<()> {
    match result {
        Err(_) => Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "write channel closed",
        )),
        Ok(Ok(Packet::Status(s))) if s.status_code == StatusCode::Ok => Ok(()),
        Ok(Ok(Packet::Status(s))) => Err(io::Error::other(s.error_message)),
        Ok(Ok(_)) => Err(io::Error::other("unexpected response packet")),
        Ok(Err(e)) => Err(io::Error::other(e.to_string())),
    }
}

fn check_write_packet(result: SftpResult<Packet>) -> SftpResult<()> {
    match result {
        Ok(Packet::Status(status)) if status.status_code == StatusCode::Ok => Ok(()),
        Ok(Packet::Status(status)) => Err(status.into()),
        Ok(_) => Err(Error::UnexpectedPacket),
        Err(error) => Err(error),
    }
}

fn poll_oldest_write(
    pending: &mut VecDeque<PendingRequest>,
    cx: &mut Context<'_>,
) -> Option<Poll<io::Result<()>>> {
    let rx = pending.front_mut()?;
    Some(match Pin::new(rx).poll(cx) {
        Poll::Pending => Poll::Pending,
        Poll::Ready(r) => {
            pending.pop_front();
            Poll::Ready(check_write_result(r))
        }
    })
}

fn poll_drain_writes(
    pending: &mut VecDeque<PendingRequest>,
    cx: &mut Context<'_>,
) -> Poll<io::Result<()>> {
    while let Some(poll) = poll_oldest_write(pending, cx) {
        ready!(poll)?;
    }
    Poll::Ready(Ok(()))
}

impl Drop for File {
    fn drop(&mut self) {
        if self.closed {
            return;
        }

        let _ = self.session.close_nowait(mem::take(&mut self.handle));
    }
}

impl AsyncRead for File {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let poll = Pin::new(match self.state.f_read.as_mut() {
            Some(f) => f,
            None => {
                let session = self.session.clone();
                let max_read_len = self
                    .features
                    .limits
                    .and_then(|l| l.read_len)
                    .unwrap_or_else(|| {
                        self.features
                            .max_packet_len
                            .saturating_sub(READ_OVERHEAD_LENGTH) as u64
                    }) as usize;

                let file_handle = self.handle.clone();

                let offset = self.pos;
                let len = usize::min(buf.remaining(), max_read_len);

                self.state.f_read.get_or_insert(Box::pin(async move {
                    let result = session.read(file_handle, offset, len as u32).await;
                    match result {
                        Ok(data) => Ok(Some(data.data)),
                        Err(Error::Status(status)) if status.status_code == StatusCode::Eof => {
                            Ok(None)
                        }
                        Err(e) => Err(io::Error::other(e.to_string())),
                    }
                }))
            }
        })
        .poll(cx);

        if poll.is_ready() {
            self.state.f_read = None;
        }

        match poll {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Ready(Ok(None)) => Poll::Ready(Ok(())),
            Poll::Ready(Ok(Some(data))) => {
                self.pos += data.len() as u64;
                buf.put_slice(&data[..]);
                Poll::Ready(Ok(()))
            }
        }
    }
}

impl AsyncSeek for File {
    fn start_seek(mut self: Pin<&mut Self>, position: io::SeekFrom) -> io::Result<()> {
        if self.state.f_seek.is_some() {
            return Err(io::Error::other(
                "other file operation is pending, call poll_complete before start_seek",
            ));
        }

        self.state.f_seek = Some(match position {
            SeekFrom::Start(pos) => Box::pin(future::ready(Ok(pos))),
            SeekFrom::Current(pos) => {
                let new_pos = self.pos as i64 + pos;
                if new_pos < 0 {
                    return Err(io::Error::other(
                        "cannot move file pointer before the beginning",
                    ));
                }
                Box::pin(future::ready(Ok(new_pos as u64)))
            }
            SeekFrom::End(pos) => {
                let session = self.session.clone();
                let file_handle = self.handle.clone();

                Box::pin(async move {
                    let result = session
                        .fstat(file_handle)
                        .await
                        .map_err(|e| io::Error::other(e.to_string()))?;
                    match result.attrs.size {
                        Some(size) => {
                            let new_pos = size as i64 + pos;
                            if new_pos < 0 {
                                return Err(io::Error::other(
                                    "cannot move file pointer before the beginning",
                                ));
                            }
                            Ok(new_pos as u64)
                        }
                        None => Err(io::Error::other("file size unknown")),
                    }
                })
            }
        });

        Ok(())
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        match self.state.f_seek.as_mut() {
            None => Poll::Ready(Ok(self.pos)),
            Some(f) => {
                self.pos = ready!(Pin::new(f).poll(cx))?;
                self.state.f_seek = None;
                Poll::Ready(Ok(self.pos))
            }
        }
    }
}

impl AsyncWrite for File {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        if self.state.write_acks.len() >= self.features.max_concurrent_writes {
            if let Some(poll) = poll_oldest_write(&mut self.state.write_acks, cx) {
                ready!(poll)?;
            }
        }

        let max_write_len = self
            .features
            .limits
            .and_then(|l| l.write_len)
            .unwrap_or_else(|| {
                let overhead = WRITE_OVERHEAD_LENGTH + self.handle.len() as u32;
                self.features.max_packet_len.saturating_sub(overhead) as u64
            }) as usize;

        let len = usize::min(buf.len(), max_write_len);
        let offset = self.pos;

        match self
            .session
            .try_write_nowait_raw(&self.handle, offset, &buf[..len])
        {
            Ok(rx) => {
                self.pos += len as u64;
                self.state.write_acks.push_back(rx);
                Poll::Ready(Ok(len))
            }
            Err(TryQueueError::Full { required_bytes }) => {
                // The regular AsyncWrite API cannot await byte capacity here.
                // Register its task and retry when the live session releases
                // enough queued-or-unacknowledged frame budget.
                self.session
                    .register_outbound_capacity_waker(required_bytes, cx);
                Poll::Pending
            }
            Err(TryQueueError::Sftp(error)) => {
                Poll::Ready(Err(io::Error::other(error.to_string())))
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        ready!(poll_drain_writes(&mut self.state.write_acks, cx))?;

        if !self.features.fsync {
            return Poll::Ready(Ok(()));
        }

        let poll = Pin::new(match self.state.f_flush.as_mut() {
            Some(f) => f,
            None => {
                let session = self.session.clone();
                let file_handle = self.handle.clone();

                self.state.f_flush.get_or_insert(Box::pin(async move {
                    session
                        .fsync(file_handle)
                        .await
                        .map(|_| ())
                        .map_err(|e| io::Error::other(e.to_string()))
                }))
            }
        })
        .poll(cx);

        if poll.is_ready() {
            self.state.f_flush = None;
        }

        poll
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        ready!(poll_drain_writes(&mut self.state.write_acks, cx))?;

        let poll = Pin::new(match self.state.f_shutdown.as_mut() {
            Some(f) => f,
            None => {
                let session = self.session.clone();
                let file_handle = self.handle.clone();

                self.state.f_shutdown.get_or_insert(Box::pin(async move {
                    session
                        .close(file_handle)
                        .await
                        .map_err(|e| io::Error::other(e.to_string()))?;
                    Ok(())
                }))
            }
        })
        .poll(cx);

        if poll.is_ready() {
            self.state.f_shutdown = None;
            self.closed = true;
        }

        poll
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_tuner_counts_short_read_without_reducing_window() {
        let mut tuner = SftpWindowTuner::new(64, 8 * 1024 * 1024, 2 * 1024 * 1024);
        let initial_requests = tuner.target_requests();
        let initial_inflight = tuner.target_inflight_bytes();
        let initial_chunk = tuner.target_chunk_len();

        tuner.record_short_read(128 * 1024, Duration::from_millis(40));

        assert_eq!(tuner.target_requests(), initial_requests);
        assert_eq!(tuner.target_inflight_bytes(), initial_inflight);
        assert_eq!(tuner.target_chunk_len(), initial_chunk);
        let snapshot = tuner.snapshot();
        assert_eq!(snapshot.short_read_count, 1);
        assert_eq!(snapshot.window_shrink_count, 0);
        assert!(snapshot.last_shrink_reason.is_none());
        assert_eq!(snapshot.completed_bytes, 128 * 1024);
    }

    #[test]
    fn window_tuner_starts_conservatively_and_fast_ramps_clean_high_rtt() {
        let mut tuner = SftpWindowTuner::new(64, 16 * 1024 * 1024, 2 * 1024 * 1024);

        assert_eq!(tuner.target_requests(), WINDOW_TUNER_START_REQUESTS);
        assert_eq!(tuner.target_chunk_len(), WINDOW_TUNER_START_CHUNK);
        assert_eq!(
            tuner.target_inflight_bytes(),
            WINDOW_TUNER_START_REQUESTS * WINDOW_TUNER_START_CHUNK
        );

        tuner.interval_started = Instant::now() - WINDOW_TUNER_ADJUST_INTERVAL;
        tuner.record_success(1024 * 1024, Duration::from_millis(250));

        assert!(tuner.target_requests() > WINDOW_TUNER_START_REQUESTS);
        assert!(
            tuner.target_inflight_bytes() > WINDOW_TUNER_START_REQUESTS * WINDOW_TUNER_START_CHUNK
        );

        let grown_requests = tuner.target_requests();
        tuner.record_error(SftpWindowShrinkReason::StatusError);

        assert!(tuner.target_requests() < grown_requests);
        assert_eq!(tuner.clean_growth_intervals, 0);
    }

    #[test]
    fn window_tuner_never_exceeds_server_caps() {
        let mut tuner = SftpWindowTuner::new(8, 512 * 1024, 128 * 1024);

        for _ in 0..16 {
            tuner.interval_started = Instant::now() - WINDOW_TUNER_ADJUST_INTERVAL;
            tuner.record_success(4 * 1024 * 1024, Duration::from_millis(250));
        }

        assert!(tuner.target_requests() <= 8);
        assert!(tuner.target_inflight_bytes() <= 512 * 1024);
        assert!(tuner.target_chunk_len() <= 128 * 1024);
    }

    fn test_downloader(
        offset: u64,
        end_offset: Option<u64>,
        max_read_len: usize,
        max_requests: usize,
    ) -> (PipelinedFileDownloader, tokio::io::DuplexStream) {
        let (client_stream, server_stream) = tokio::io::duplex(4096);
        let parts = FileDownloadParts {
            session: Arc::new(RawSftpSession::new(client_stream)),
            handle: "test-handle".to_owned(),
            max_read_len,
            closed: false,
        };
        let downloader = PipelinedFileDownloader::new(
            parts,
            offset,
            end_offset,
            max_requests,
            max_read_len.saturating_mul(max_requests),
        );
        (downloader, server_stream)
    }

    fn pending_read(
        offset: u64,
        requested_len: usize,
    ) -> (PendingRead, oneshot::Sender<SftpResult<Packet>>) {
        let (tx, rx) = oneshot::channel();
        (
            PendingRead {
                offset,
                requested_len,
                sent_at: Instant::now(),
                rx: PendingRequest::from_test_receiver(rx),
            },
            tx,
        )
    }

    fn completed_data(offset: u64, requested_len: usize, data: Bytes) -> CompletedRead {
        CompletedRead {
            offset,
            requested_len,
            sent_at: Instant::now(),
            result: Ok(Packet::Data(crate::protocol::Data { id: 1, data })),
        }
    }

    #[tokio::test]
    async fn short_read_repairs_only_missing_tail_and_keeps_out_of_order_work() {
        const CHUNK_LEN: usize = 128;
        const SHORT_READ_LEN: usize = 32;
        const END_OFFSET: u64 = 512;

        let (mut downloader, _server_stream) = test_downloader(0, Some(END_OFFSET), CHUNK_LEN, 4);
        let initial_requests = downloader.tuner.target_requests();
        let initial_inflight = downloader.tuner.target_inflight_bytes();
        let initial_chunk = downloader.tuner.target_chunk_len();

        // A later response is already buffered while unrelated requests remain in flight.
        downloader
            .process_completed_read(completed_data(
                256,
                CHUNK_LEN,
                Bytes::from(vec![3; CHUNK_LEN]),
            ))
            .expect("out-of-order read should be buffered");
        let (pending_before_gap, _pending_before_gap_tx) = pending_read(128, CHUNK_LEN);
        let (pending_after_gap, _pending_after_gap_tx) = pending_read(384, CHUNK_LEN);
        downloader.pending.push_back(pending_before_gap);
        downloader.pending.push_back(pending_after_gap);
        downloader.inflight_bytes = CHUNK_LEN * 2;
        downloader.next_request_offset = END_OFFSET;

        downloader
            .process_completed_read(completed_data(
                0,
                CHUNK_LEN,
                Bytes::from(vec![1; SHORT_READ_LEN]),
            ))
            .expect("legal short read should be accepted");

        assert_eq!(downloader.pending.len(), 2);
        assert!(downloader.ready_chunks.contains_key(&0));
        assert!(downloader.ready_chunks.contains_key(&256));
        assert_eq!(
            downloader.repair_reads.get(&(SHORT_READ_LEN as u64)),
            Some(&(CHUNK_LEN - SHORT_READ_LEN))
        );
        assert_eq!(downloader.next_request_offset, END_OFFSET);

        let snapshot = downloader.diagnostic_snapshot();
        assert_eq!(snapshot.discarded_speculative_requests, 0);
        assert_eq!(snapshot.discarded_ready_chunks, 0);
        assert_eq!(snapshot.restart_from_offset_count, 0);
        assert_eq!(snapshot.out_of_order_completed, 1);
        assert_eq!(snapshot.window.short_read_count, 1);
        assert_eq!(snapshot.window.window_shrink_count, 0);
        assert_eq!(snapshot.window.target_requests, initial_requests);
        assert_eq!(snapshot.window.target_inflight_bytes, initial_inflight);
        assert_eq!(snapshot.window.target_chunk_len, initial_chunk);

        downloader.fill_pending().await;

        assert!(downloader.repair_reads.is_empty());
        assert!(downloader.pending.iter().any(|pending| {
            pending.offset == SHORT_READ_LEN as u64
                && pending.requested_len == CHUNK_LEN - SHORT_READ_LEN
        }));
        assert!(downloader
            .pending
            .iter()
            .any(|pending| pending.offset == 128 && pending.requested_len == CHUNK_LEN));
        assert!(downloader
            .pending
            .iter()
            .any(|pending| pending.offset == 384 && pending.requested_len == CHUNK_LEN));
    }

    #[tokio::test]
    async fn known_length_resume_reports_early_eof() {
        const RESUME_OFFSET: u64 = 128;
        const EXPECTED_END_OFFSET: u64 = 512;
        const EOF_OFFSET: u64 = 384;
        const CHUNK_LEN: usize = 128;

        let (mut downloader, _server_stream) =
            test_downloader(RESUME_OFFSET, Some(EXPECTED_END_OFFSET), CHUNK_LEN, 4);
        let (pending, _pending_tx) = pending_read(RESUME_OFFSET, CHUNK_LEN);
        downloader.pending.push_back(pending);
        downloader.inflight_bytes = CHUNK_LEN;
        let initial_requests = downloader.tuner.target_requests();
        let initial_inflight = downloader.tuner.target_inflight_bytes();

        let error = downloader
            .process_completed_read(CompletedRead {
                offset: EOF_OFFSET,
                requested_len: CHUNK_LEN,
                sent_at: Instant::now(),
                result: Ok(Packet::Status(crate::protocol::Status {
                    id: 1,
                    status_code: StatusCode::Eof,
                    error_message: String::new(),
                    language_tag: String::new(),
                })),
            })
            .expect_err("EOF before a known end offset must fail the download");

        assert!(matches!(
            error,
            Error::UnexpectedBehavior(message)
                if message.contains("offset 384") && message.contains("end offset 512")
        ));
        assert!(downloader.finished);
        assert!(downloader.pending.is_empty());
        assert_eq!(downloader.inflight_bytes, 0);
        assert_eq!(downloader.tuner.target_requests(), initial_requests);
        assert_eq!(downloader.tuner.target_inflight_bytes(), initial_inflight);
    }
}
