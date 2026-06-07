use std::{
    collections::{BTreeMap, VecDeque},
    future::{self, Future},
    io::{self, SeekFrom},
    mem,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf},
    runtime::Handle,
    sync::oneshot,
};

use super::Metadata;
use crate::{
    client::{error::Error, rawsession::SftpResult, session::Features, RawSftpSession},
    protocol::{Packet, StatusCode},
};

type StateFn<T> = Option<Pin<Box<dyn Future<Output = io::Result<T>> + Send + Sync + 'static>>>;

// read packet overhead: type(1) + id(4) + data_len(4)
const READ_OVERHEAD_LENGTH: u32 = 9;
// write packet overhead excluding handle: type(1) + id(4) + handle_len(4) + offset(8) + data_len(4)
const WRITE_OVERHEAD_LENGTH: u32 = 21;

struct FileState {
    f_read: StateFn<Option<Vec<u8>>>,
    f_seek: StateFn<u64>,
    f_flush: StateFn<()>,
    f_shutdown: StateFn<()>,
    write_acks: VecDeque<oneshot::Receiver<SftpResult<Packet>>>,
}

struct PendingRead {
    offset: u64,
    requested_len: usize,
    rx: oneshot::Receiver<SftpResult<Packet>>,
}

struct CompletedRead {
    offset: u64,
    requested_len: usize,
    result: SftpResult<Packet>,
}

struct PendingWrite {
    requested_len: usize,
    rx: oneshot::Receiver<SftpResult<Packet>>,
}

/// A chunk returned by a pipelined SFTP file read.
pub struct PipelinedReadChunk {
    pub offset: u64,
    pub data: Vec<u8>,
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
    ready_chunks: BTreeMap<u64, Vec<u8>>,
    next_request_offset: u64,
    next_write_offset: u64,
    end_offset: Option<u64>,
    max_read_len: usize,
    max_requests: usize,
    max_inflight_bytes: usize,
    inflight_bytes: usize,
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
    max_requests: usize,
    max_inflight_bytes: usize,
    inflight_bytes: usize,
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

        if let Ok(handle) = Handle::try_current() {
            let session = self.session.clone();
            let file_handle = self.handle.clone();

            handle.spawn(async move {
                let _ = session.close(file_handle).await;
            });
        }
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

        if let Ok(handle) = Handle::try_current() {
            let session = self.session.clone();
            let file_handle = self.handle.clone();

            handle.spawn(async move {
                let _ = session.close(file_handle).await;
            });
        }
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
        Self {
            session,
            handle,
            pending: VecDeque::with_capacity(max_requests),
            ready_chunks: BTreeMap::new(),
            next_request_offset: offset,
            next_write_offset: offset,
            end_offset,
            max_read_len,
            max_requests,
            max_inflight_bytes,
            inflight_bytes: 0,
            scheduling_error: None,
            finished: false,
            closed: false,
        }
    }

    fn fill_pending(&mut self) {
        if self.finished {
            return;
        }

        while self.pending.len() < self.max_requests
            && self.inflight_bytes < self.max_inflight_bytes
        {
            if self
                .end_offset
                .is_some_and(|end_offset| self.next_request_offset >= end_offset)
            {
                self.finished = true;
                break;
            }

            let offset = self.next_request_offset;
            let remaining_inflight = self.max_inflight_bytes.saturating_sub(self.inflight_bytes);
            let requested_len = self
                .end_offset
                .map(|end_offset| {
                    usize::try_from(end_offset.saturating_sub(offset)).unwrap_or(usize::MAX)
                })
                .map(|remaining| remaining.min(self.max_read_len))
                .unwrap_or(self.max_read_len)
                .min(remaining_inflight)
                .max(1);
            let rx =
                match self
                    .session
                    .read_nowait(self.handle.clone(), offset, requested_len as u32)
                {
                    Ok(rx) => rx,
                    Err(error) => {
                        self.scheduling_error = Some(error);
                        self.finished = true;
                        self.discard_pending();
                        break;
                    }
                };

            self.next_request_offset = self
                .next_request_offset
                .saturating_add(requested_len as u64);
            self.inflight_bytes = self.inflight_bytes.saturating_add(requested_len);
            self.pending.push_back(PendingRead {
                offset,
                requested_len,
                rx,
            });
        }
    }

    fn discard_pending(&mut self) {
        for pending in self.pending.drain(..) {
            self.inflight_bytes = self.inflight_bytes.saturating_sub(pending.requested_len);
        }
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
            result,
        }))
    }

    fn process_completed_read(&mut self, completed: CompletedRead) -> SftpResult<()> {
        match completed.result {
            Ok(Packet::Data(data)) => {
                let read_len = data.data.len();
                if read_len == 0 {
                    self.finished = true;
                    self.discard_pending();
                    return Ok(());
                }

                if read_len < completed.requested_len {
                    let next_offset = completed.offset.saturating_add(read_len as u64);
                    self.next_request_offset = next_offset;
                    self.discard_pending();
                    self.ready_chunks.retain(|offset, _| *offset < next_offset);
                }

                self.ready_chunks.insert(completed.offset, data.data);
                Ok(())
            }
            Ok(Packet::Status(status)) if status.status_code == StatusCode::Eof => {
                self.finished = true;
                self.discard_pending();
                Ok(())
            }
            Ok(Packet::Status(status)) => {
                self.discard_pending();
                Err(status.into())
            }
            Ok(_) => {
                self.discard_pending();
                Err(Error::UnexpectedPacket)
            }
            Err(error) => {
                self.discard_pending();
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
            self.fill_pending();

            if let Some(data) = self.ready_chunks.remove(&self.next_write_offset) {
                let offset = self.next_write_offset;
                self.next_write_offset = self.next_write_offset.saturating_add(data.len() as u64);
                return Ok(Some(PipelinedReadChunk { offset, data }));
            }

            if let Some(error) = self.scheduling_error.take() {
                return Err(error);
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

        if let Ok(handle) = Handle::try_current() {
            let session = self.session.clone();
            let file_handle = self.handle.clone();

            handle.spawn(async move {
                let _ = session.close(file_handle).await;
            });
        }
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
        Self {
            session,
            handle,
            pending: VecDeque::with_capacity(max_requests),
            next_write_offset: offset,
            max_write_len,
            max_requests,
            max_inflight_bytes,
            inflight_bytes: 0,
            fsync,
            closed: false,
        }
    }

    fn has_write_capacity(&self) -> bool {
        self.pending.len() < self.max_requests && self.inflight_bytes < self.max_inflight_bytes
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
                Ok(result) => check_write_packet(result).map(|_| pending.requested_len),
                Err(_) => Err(Error::UnexpectedBehavior("write channel closed".into())),
            }?;
            completed_bytes = completed_bytes.saturating_add(completed);

            if self.pending.is_empty() {
                return Poll::Ready(Ok(completed_bytes));
            }
        }
    }

    async fn wait_for_capacity(&mut self) -> SftpResult<()> {
        while !self.has_write_capacity() {
            let _completed = future::poll_fn(|cx| self.poll_write_progress(cx, true)).await?;
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

            let remaining_inflight = self.max_inflight_bytes.saturating_sub(self.inflight_bytes);
            let len = data.len().min(self.max_write_len).min(remaining_inflight);
            if len == 0 {
                self.wait_for_capacity().await?;
                continue;
            }

            let offset = self.next_write_offset;
            let payload = data[..len].to_vec();
            let rx = self
                .session
                .write_nowait(self.handle.clone(), offset, payload)?;
            self.pending.push_back(PendingWrite {
                requested_len: len,
                rx,
            });
            self.inflight_bytes = self.inflight_bytes.saturating_add(len);
            self.next_write_offset = self.next_write_offset.saturating_add(len as u64);
            scheduled = scheduled.saturating_add(len);
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
}

impl Drop for PipelinedFileUploader {
    fn drop(&mut self) {
        self.pending.clear();
        self.inflight_bytes = 0;
        if self.closed {
            return;
        }

        if let Ok(handle) = Handle::try_current() {
            let session = self.session.clone();
            let file_handle = self.handle.clone();

            handle.spawn(async move {
                let _ = session.close(file_handle).await;
            });
        }
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
    pending: &mut VecDeque<oneshot::Receiver<SftpResult<Packet>>>,
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
    pending: &mut VecDeque<oneshot::Receiver<SftpResult<Packet>>>,
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

        if let Ok(handle) = Handle::try_current() {
            let session = self.session.clone();
            let file_handle = self.handle.clone();

            handle.spawn(async move {
                let _ = session.close(file_handle).await;
            });
        }
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
        let data = buf[..len].to_vec();
        let handle = self.handle.clone();
        let offset = self.pos;

        match self.session.write_nowait(handle, offset, data) {
            Ok(rx) => {
                self.pos += len as u64;
                self.state.write_acks.push_back(rx);
                Poll::Ready(Ok(len))
            }
            Err(e) => Poll::Ready(Err(io::Error::other(e.to_string()))),
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
