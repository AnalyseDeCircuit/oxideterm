// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::VecDeque,
    future::Future,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use bytes::{BufMut, Bytes, BytesMut};
use russh::ChannelMsg;
use tokio::sync::mpsc;
use tracing::debug;

use crate::{
    SftpError, SftpTransferGuard, SftpTransferManager, TransferDirection, TransferProgress,
    TransferState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TarCompression {
    None,
    Zstd,
    Gzip,
}

impl TarCompression {
    fn tar_flag(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Zstd => " --zstd",
            Self::Gzip => " -z",
        }
    }
}

/// Remote archive commands available to one live SSH connection generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TarCapabilities {
    pub supports_tar: bool,
    pub compression: TarCompression,
}

impl TarCapabilities {
    /// Represents a remote environment where tar transfers are unavailable.
    pub const fn unsupported() -> Self {
        Self {
            supports_tar: false,
            compression: TarCompression::None,
        }
    }
}

// Archive setup is worthwhile for several files, while two small files already
// benefit from avoiding repeated remote open and close round trips.
const TAR_MIN_FILE_COUNT: u64 = 8;
const TAR_SMALL_FILE_AVERAGE_BYTES: u64 = 256 * 1024;
const TAR_ALREADY_COMPRESSED_PERCENT: u64 = 80;
const TAR_MAX_RECURSION_DEPTH: u32 = 64;
const TAR_MAX_STDERR_BYTES: usize = 64 * 1024;
const TAR_EXIT_TIMEOUT: Duration = Duration::from_secs(15);
const TAR_CHANNEL_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const TAR_CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Regular-file metadata used to choose transport and compression without
/// reading file contents into memory.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TarDirectoryProfile {
    pub total_bytes: u64,
    pub file_count: u64,
    already_compressed_bytes: u64,
}

impl TarDirectoryProfile {
    pub fn record_file(&mut self, path: &Path, size: u64) {
        self.total_bytes = self.total_bytes.saturating_add(size);
        self.file_count = self.file_count.saturating_add(1);
        if is_likely_compressed(path) {
            self.already_compressed_bytes = self.already_compressed_bytes.saturating_add(size);
        }
    }

    pub fn prefers_tar(self) -> bool {
        if self.file_count >= TAR_MIN_FILE_COUNT {
            return true;
        }
        self.file_count >= 2 && self.total_bytes / self.file_count <= TAR_SMALL_FILE_AVERAGE_BYTES
    }

    pub fn recommended_compression(self, available: TarCompression) -> TarCompression {
        if available == TarCompression::None || self.total_bytes == 0 {
            return TarCompression::None;
        }
        let compressed_percent =
            self.already_compressed_bytes.saturating_mul(100) / self.total_bytes;
        if compressed_percent >= TAR_ALREADY_COMPRESSED_PERCENT {
            TarCompression::None
        } else {
            available
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TarTransferResult {
    /// Bytes carried by the compressed or uncompressed archive stream.
    pub stream_bytes: u64,
    pub item_count: u64,
}

/// Immutable inputs shared by archive upload and download after strategy selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TarTransferOptions {
    pub profile: TarDirectoryProfile,
    pub compression: TarCompression,
}

pub trait SftpExecChannelOpener: Clone + Send + Sync + 'static {
    fn open_exec_channel(
        &self,
    ) -> impl Future<Output = Result<russh::Channel<russh::client::Msg>, SftpError>> + Send;
}

pub async fn probe_tar_support<O>(opener: &O) -> bool
where
    O: SftpExecChannelOpener,
{
    probe_exec_exit0(opener, "tar --version").await
}

pub async fn probe_tar_compression<O>(opener: &O) -> TarCompression
where
    O: SftpExecChannelOpener,
{
    if probe_exec_exit0(opener, "tar --zstd -cf /dev/null /dev/null 2>/dev/null").await {
        return TarCompression::Zstd;
    }
    if probe_exec_exit0(opener, "tar -zcf /dev/null /dev/null 2>/dev/null").await {
        return TarCompression::Gzip;
    }
    TarCompression::None
}

/// Probes tar support and the best available compression command once.
pub async fn probe_tar_capabilities<O>(opener: &O) -> TarCapabilities
where
    O: SftpExecChannelOpener,
{
    if !probe_tar_support(opener).await {
        return TarCapabilities::unsupported();
    }

    TarCapabilities {
        supports_tar: true,
        compression: probe_tar_compression(opener).await,
    }
}

pub async fn tar_upload_directory<O>(
    opener: &O,
    local_path: &str,
    remote_path: &str,
    transfer_id: &str,
    progress_tx: Option<mpsc::Sender<TransferProgress>>,
    transfer_manager: Option<Arc<SftpTransferManager>>,
    options: TarTransferOptions,
) -> Result<TarTransferResult, SftpError>
where
    O: SftpExecChannelOpener,
{
    let TarTransferOptions {
        profile,
        compression,
    } = options;
    let _control = transfer_manager
        .as_ref()
        .map(|manager| manager.register(transfer_id));
    let _control_guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
    if let Some(manager) = &transfer_manager {
        manager.check_control(transfer_id).await?;
    }
    let local = Path::new(local_path);
    if !local.is_dir() {
        return Err(SftpError::DirectoryNotFound(local_path.to_string()));
    }
    let mut channel = opener.open_exec_channel().await?;
    let cmd = format!(
        "tar{} -xf - -C {}",
        compression.tar_flag(),
        shell_escape(remote_path)
    );
    debug!("tar upload exec: {cmd}");
    if let Err(error) = request_tar_exec(&channel, cmd, transfer_id, &transfer_manager).await {
        let _ = channel.close().await;
        return Err(error);
    }

    let (data_tx, mut data_rx) = mpsc::channel::<Bytes>(32);
    let processed_bytes = Arc::new(AtomicU64::new(0));
    let processed_items = Arc::new(AtomicU64::new(0));
    // tar::Builder is synchronous. Keep it on a blocking thread and bridge it
    // to the async SSH channel with bounded chunks, matching the Tauri pipeline.
    let tar_handle = tokio::task::spawn_blocking({
        let local_path = local_path.to_string();
        let processed_bytes = processed_bytes.clone();
        let processed_items = processed_items.clone();
        move || {
            tar_encode_directory(
                &local_path,
                data_tx,
                compression,
                &processed_bytes,
                &processed_items,
            )
        }
    });

    let start = Instant::now();
    let mut sent = 0u64;
    let mut last_progress = Instant::now();
    loop {
        let chunk = match receive_encoder_chunk(&mut data_rx, transfer_id, &transfer_manager).await
        {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                let _ = channel.close().await;
                // Closing the receiver releases a producer blocked by bounded backpressure.
                drop(data_rx);
                let _ = tar_handle.await;
                return Err(error);
            }
        };
        // Capture the byte count before handing the owned allocation to russh.
        let chunk_len = chunk.len() as u64;
        if let Err(error) = send_channel_chunk(
            &channel,
            chunk,
            transfer_id,
            &transfer_manager,
            TAR_CHANNEL_IDLE_TIMEOUT,
        )
        .await
        {
            drop(data_rx);
            let _ = tar_handle.await;
            let _ = channel.close().await;
            return Err(error);
        }
        sent += chunk_len;
        if let Err(error) = throttle(sent, start, transfer_id, &transfer_manager).await {
            let _ = channel.close().await;
            drop(data_rx);
            let _ = tar_handle.await;
            return Err(error);
        }
        if last_progress.elapsed().as_millis() >= 200 {
            let processed = processed_bytes.load(Ordering::Relaxed);
            send_progress(
                &progress_tx,
                transfer_id,
                remote_path,
                local_path,
                TransferDirection::Upload,
                profile.total_bytes,
                processed.min(profile.total_bytes),
                start,
                TransferState::InProgress,
            )
            .await;
            last_progress = Instant::now();
        }
    }
    let build_result = tar_handle
        .await
        .map_err(|error| SftpError::TransferError(format!("tar builder panicked: {error}")))?;
    if let Err(error) = build_result {
        let _ = channel.close().await;
        return Err(error);
    }
    channel
        .eof()
        .await
        .map_err(|error| SftpError::ChannelError(format!("Failed to send EOF: {error}")))?;
    let exit_result = drain_channel_exit_with_control(
        &mut channel,
        transfer_id,
        &transfer_manager,
        TAR_EXIT_TIMEOUT,
    )
    .await;
    let _ = channel.close().await;
    let exit = exit_result?;
    validate_exit(exit)?;
    let completed_bytes = processed_bytes.load(Ordering::Relaxed);
    let completed_items = processed_items.load(Ordering::Relaxed);
    send_progress(
        &progress_tx,
        transfer_id,
        remote_path,
        local_path,
        TransferDirection::Upload,
        completed_bytes,
        completed_bytes,
        start,
        TransferState::Completed,
    )
    .await;
    Ok(TarTransferResult {
        stream_bytes: sent,
        item_count: completed_items,
    })
}

pub async fn tar_download_directory<O>(
    opener: &O,
    remote_path: &str,
    local_path: &str,
    transfer_id: &str,
    progress_tx: Option<mpsc::Sender<TransferProgress>>,
    transfer_manager: Option<Arc<SftpTransferManager>>,
    options: TarTransferOptions,
) -> Result<TarTransferResult, SftpError>
where
    O: SftpExecChannelOpener,
{
    let TarTransferOptions {
        profile,
        compression,
    } = options;
    let _control = transfer_manager
        .as_ref()
        .map(|manager| manager.register(transfer_id));
    let _control_guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
    if let Some(manager) = &transfer_manager {
        manager.check_control(transfer_id).await?;
    }
    tokio::fs::create_dir_all(local_path)
        .await
        .map_err(SftpError::IoError)?;
    let mut channel = opener.open_exec_channel().await?;
    let cmd = format!(
        "tar{} -cf - -C {} .",
        compression.tar_flag(),
        shell_escape(remote_path)
    );
    debug!("tar download exec: {cmd}");
    if let Err(error) = request_tar_exec(&channel, cmd, transfer_id, &transfer_manager).await {
        let _ = channel.close().await;
        return Err(error);
    }

    let start = Instant::now();
    let (data_tx, data_rx) = mpsc::channel::<Bytes>(64);
    let processed_bytes = Arc::new(AtomicU64::new(0));
    let decode_handle = tokio::task::spawn_blocking({
        let local_path = local_path.to_string();
        let processed_bytes = processed_bytes.clone();
        move || tar_decode_directory(&local_path, data_rx, compression, &processed_bytes)
    });

    let mut stderr = Vec::new();
    let mut exit_code = None;
    let mut received = 0u64;
    let mut last_progress = Instant::now();
    let receive_result = async {
        loop {
            match wait_channel_message(
                &mut channel,
                transfer_id,
                &transfer_manager,
                TAR_CHANNEL_IDLE_TIMEOUT,
            )
            .await?
            {
                Some(ChannelMsg::Data { data: chunk }) => {
                    received += chunk.len() as u64;
                    send_decoder_chunk(&data_tx, chunk, transfer_id, &transfer_manager).await?;
                    throttle(received, start, transfer_id, &transfer_manager).await?;
                    if last_progress.elapsed().as_millis() >= 200 {
                        let processed = processed_bytes.load(Ordering::Relaxed);
                        send_progress(
                            &progress_tx,
                            transfer_id,
                            remote_path,
                            local_path,
                            TransferDirection::Download,
                            profile.total_bytes,
                            processed.min(profile.total_bytes),
                            start,
                            TransferState::InProgress,
                        )
                        .await;
                        last_progress = Instant::now();
                    }
                }
                Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                    append_bounded(&mut stderr, &data, TAR_MAX_STDERR_BYTES)
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => exit_code = Some(exit_status),
                Some(ChannelMsg::Eof) => {}
                Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
        Ok::<(), SftpError>(())
    }
    .await;
    drop(data_tx);
    // The async channel owner always joins the blocking decoder after closing
    // its input, including cancellation, timeout, and protocol failures.
    let decode_result = decode_handle
        .await
        .map_err(|error| SftpError::TransferError(format!("tar decoder panicked: {error}")))?;
    let _ = channel.close().await;
    receive_result?;
    let decoded = decode_result?;
    validate_exit(ExecExit {
        exit_code,
        stderr,
        timed_out: false,
    })?;
    let local_path = local_path.to_string();
    send_progress(
        &progress_tx,
        transfer_id,
        remote_path,
        &local_path,
        TransferDirection::Download,
        decoded.unpacked_bytes,
        decoded.unpacked_bytes,
        start,
        TransferState::Completed,
    )
    .await;
    Ok(TarTransferResult {
        stream_bytes: received,
        item_count: decoded.item_count,
    })
}

async fn probe_exec_exit0<O>(opener: &O, command: &str) -> bool
where
    O: SftpExecChannelOpener,
{
    let Ok(mut channel) = opener.open_exec_channel().await else {
        return false;
    };
    if !matches!(
        tokio::time::timeout(TAR_EXIT_TIMEOUT, channel.exec(true, command)).await,
        Ok(Ok(()))
    ) {
        let _ = channel.close().await;
        return false;
    }
    let exit = drain_channel_exit_with_timeout(&mut channel, Duration::from_secs(10)).await;
    let _ = channel.close().await;
    !exit.timed_out && exit.exit_code == Some(0)
}

async fn receive_encoder_chunk(
    data_rx: &mut mpsc::Receiver<Bytes>,
    transfer_id: &str,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
) -> Result<Option<Bytes>, SftpError> {
    loop {
        if let Some(manager) = transfer_manager {
            manager.check_control(transfer_id).await?;
        }
        if let Ok(chunk) = tokio::time::timeout(TAR_CONTROL_POLL_INTERVAL, data_rx.recv()).await {
            return Ok(chunk);
        }
    }
}

async fn request_tar_exec(
    channel: &russh::Channel<russh::client::Msg>,
    command: String,
    transfer_id: &str,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
) -> Result<(), SftpError> {
    let mut request = Box::pin(channel.exec(true, command));
    let mut started = Instant::now();
    loop {
        if let Some(manager) = transfer_manager {
            let was_paused = manager
                .get_control(transfer_id)
                .is_some_and(|control| control.is_paused());
            let control_check_started = Instant::now();
            manager.check_control(transfer_id).await?;
            if was_paused || control_check_started.elapsed() >= TAR_CONTROL_POLL_INTERVAL {
                started = Instant::now();
            }
        }
        match tokio::time::timeout(TAR_CONTROL_POLL_INTERVAL, &mut request).await {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(error)) => {
                return Err(SftpError::ChannelError(format!(
                    "Failed to exec tar: {error}"
                )));
            }
            Err(_) if started.elapsed() >= TAR_EXIT_TIMEOUT => {
                return Err(SftpError::TransferError(
                    "Remote tar did not accept the exec request before timeout".to_string(),
                ));
            }
            Err(_) => {}
        }
    }
}

async fn send_channel_chunk(
    channel: &russh::Channel<russh::client::Msg>,
    chunk: Bytes,
    transfer_id: &str,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
    idle_timeout: Duration,
) -> Result<(), SftpError> {
    let mut send = Box::pin(channel.data_bytes(chunk));
    let mut idle_started = Instant::now();
    loop {
        if let Some(manager) = transfer_manager {
            let was_paused = manager
                .get_control(transfer_id)
                .is_some_and(|control| control.is_paused());
            let control_check_started = Instant::now();
            manager.check_control(transfer_id).await?;
            if was_paused || control_check_started.elapsed() >= TAR_CONTROL_POLL_INTERVAL {
                idle_started = Instant::now();
            }
        }
        match tokio::time::timeout(TAR_CONTROL_POLL_INTERVAL, &mut send).await {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(error)) => {
                return Err(SftpError::ChannelError(format!(
                    "Failed to write tar data: {error}"
                )));
            }
            Err(_) if idle_started.elapsed() >= idle_timeout => {
                return Err(SftpError::TransferError(
                    "Remote tar channel stopped accepting data".to_string(),
                ));
            }
            Err(_) => {}
        }
    }
}

async fn send_decoder_chunk(
    data_tx: &mpsc::Sender<Bytes>,
    chunk: Bytes,
    transfer_id: &str,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
) -> Result<(), SftpError> {
    loop {
        if let Some(manager) = transfer_manager {
            manager.check_control(transfer_id).await?;
        }
        match tokio::time::timeout(TAR_CONTROL_POLL_INTERVAL, data_tx.reserve()).await {
            Ok(Ok(permit)) => {
                permit.send(chunk);
                return Ok(());
            }
            Ok(Err(_)) => {
                return Err(SftpError::TransferError(
                    "tar decoder stopped early".to_string(),
                ));
            }
            Err(_) => {}
        }
    }
}

const TAR_STREAM_CHUNK_SIZE: usize = 256 * 1024;

struct ChunkWriter {
    tx: mpsc::Sender<Bytes>,
    buffer: BytesMut,
}

impl ChunkWriter {
    fn new(tx: mpsc::Sender<Bytes>) -> Self {
        Self {
            tx,
            buffer: BytesMut::with_capacity(TAR_STREAM_CHUNK_SIZE),
        }
    }

    fn send_full_chunks(&mut self) -> std::io::Result<()> {
        while self.buffer.len() >= TAR_STREAM_CHUNK_SIZE {
            // BytesMut::split_to keeps the chunk backed by the existing buffer
            // allocation instead of copying every tar chunk into a fresh Vec.
            let chunk = self.buffer.split_to(TAR_STREAM_CHUNK_SIZE).freeze();
            self.tx.blocking_send(chunk).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::BrokenPipe, "tar stream closed")
            })?;
        }

        Ok(())
    }

    fn send_remaining(&mut self) -> std::io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let chunk = self.buffer.split().freeze();
        self.tx
            .blocking_send(chunk)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "tar stream closed"))
    }
}

impl Write for ChunkWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buffer.put_slice(data);
        self.send_full_chunks()?;
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.send_remaining()
    }
}

impl Drop for ChunkWriter {
    fn drop(&mut self) {
        let _ = self.send_remaining();
    }
}

struct ChannelReader {
    rx: mpsc::Receiver<Bytes>,
    buffer: Bytes,
    position: usize,
}

impl ChannelReader {
    fn new(rx: mpsc::Receiver<Bytes>) -> Self {
        Self {
            rx,
            buffer: Bytes::new(),
            position: 0,
        }
    }
}

impl Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        while self.position >= self.buffer.len() {
            match self.rx.blocking_recv() {
                Some(chunk) => {
                    self.buffer = chunk;
                    self.position = 0;
                }
                None => return Ok(0),
            }
        }

        let available = &self.buffer[self.position..];
        let n = available.len().min(out.len());
        out[..n].copy_from_slice(&available[..n]);
        self.position += n;
        Ok(n)
    }
}

fn tar_encode_directory(
    local_path: &str,
    data_tx: mpsc::Sender<Bytes>,
    compression: TarCompression,
    processed_bytes: &AtomicU64,
    processed_items: &AtomicU64,
) -> Result<(), SftpError> {
    fn append_tar<W: Write>(
        writer: W,
        local_path: &str,
        processed_bytes: &AtomicU64,
        processed_items: &AtomicU64,
    ) -> Result<W, SftpError> {
        let root = Path::new(local_path);
        let mut builder = tar::Builder::new(writer);
        // Match recursive SFTP uploads by never following or archiving local symlinks.
        builder.follow_symlinks(false);
        builder.mode(tar::HeaderMode::Complete);
        builder.append_dir(".", root).map_err(SftpError::IoError)?;
        let mut stack = VecDeque::from([(root.to_path_buf(), PathBuf::from("."), 0)]);
        while let Some((local_dir, archive_dir, depth)) = stack.pop_back() {
            if depth >= TAR_MAX_RECURSION_DEPTH {
                return Err(SftpError::TransferError(format!(
                    "tar upload recursion depth {TAR_MAX_RECURSION_DEPTH} reached at {}",
                    local_dir.display()
                )));
            }
            for entry in std::fs::read_dir(&local_dir).map_err(SftpError::IoError)? {
                let entry = entry.map_err(SftpError::IoError)?;
                let local_entry = entry.path();
                let archive_entry = archive_dir.join(entry.file_name());
                let metadata =
                    std::fs::symlink_metadata(&local_entry).map_err(SftpError::IoError)?;
                if metadata.file_type().is_symlink() {
                    continue;
                }
                if metadata.is_dir() {
                    builder
                        .append_dir(&archive_entry, &local_entry)
                        .map_err(SftpError::IoError)?;
                    stack.push_back((local_entry, archive_entry, depth + 1));
                } else if metadata.is_file() {
                    builder
                        .append_path_with_name(&local_entry, &archive_entry)
                        .map_err(SftpError::IoError)?;
                    processed_bytes.fetch_add(metadata.len(), Ordering::Relaxed);
                    processed_items.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        builder.into_inner().map_err(SftpError::IoError)
    }

    let writer = ChunkWriter::new(data_tx);
    match compression {
        TarCompression::None => {
            let mut writer = append_tar(writer, local_path, processed_bytes, processed_items)?;
            writer.flush().map_err(SftpError::IoError)?;
        }
        TarCompression::Gzip => {
            let encoder = flate2::write::GzEncoder::new(writer, flate2::Compression::fast());
            let encoder = append_tar(encoder, local_path, processed_bytes, processed_items)?;
            let mut writer = encoder.finish().map_err(SftpError::IoError)?;
            writer.flush().map_err(SftpError::IoError)?;
        }
        TarCompression::Zstd => {
            let encoder = zstd::Encoder::new(writer, 3).map_err(SftpError::IoError)?;
            let encoder = append_tar(encoder, local_path, processed_bytes, processed_items)?;
            let mut writer = encoder.finish().map_err(SftpError::IoError)?;
            writer.flush().map_err(SftpError::IoError)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TarDecodeResult {
    unpacked_bytes: u64,
    item_count: u64,
}

fn tar_decode_directory(
    local_path: &str,
    data_rx: mpsc::Receiver<Bytes>,
    compression: TarCompression,
    processed_bytes: &AtomicU64,
) -> Result<TarDecodeResult, SftpError> {
    fn unpack_tar<R: Read>(
        reader: R,
        local_path: &str,
        processed_bytes: &AtomicU64,
    ) -> Result<TarDecodeResult, SftpError> {
        let mut archive = tar::Archive::new(reader);
        archive.set_preserve_permissions(true);
        let destination = Path::new(local_path)
            .canonicalize()
            .map_err(SftpError::IoError)?;
        let mut directories = Vec::new();
        let mut result = TarDecodeResult::default();
        for entry in archive.entries().map_err(SftpError::IoError)? {
            let mut entry = entry.map_err(SftpError::IoError)?;
            if entry.header().entry_type().is_dir() {
                // Directory metadata is applied after children so restrictive
                // permissions cannot prevent extraction of their descendants.
                directories.push(entry);
                continue;
            }
            let is_file = entry.header().entry_type().is_file();
            let entry_size = entry.header().size().map_err(SftpError::IoError)?;
            if entry.unpack_in(&destination).map_err(SftpError::IoError)? && is_file {
                result.item_count = result.item_count.saturating_add(1);
                result.unpacked_bytes = result.unpacked_bytes.saturating_add(entry_size);
                processed_bytes.store(result.unpacked_bytes, Ordering::Relaxed);
            }
        }
        directories.sort_by(|left, right| right.path_bytes().cmp(&left.path_bytes()));
        for mut directory in directories {
            directory
                .unpack_in(&destination)
                .map_err(SftpError::IoError)?;
        }
        Ok(result)
    }

    let reader = ChannelReader::new(data_rx);
    match compression {
        TarCompression::None => unpack_tar(reader, local_path, processed_bytes),
        TarCompression::Gzip => {
            let decoder = flate2::read::GzDecoder::new(reader);
            unpack_tar(decoder, local_path, processed_bytes)
        }
        TarCompression::Zstd => {
            let decoder = zstd::Decoder::new(reader).map_err(SftpError::IoError)?;
            unpack_tar(decoder, local_path, processed_bytes)
        }
    }
}

pub async fn profile_local_directory(path: &Path) -> Result<TarDirectoryProfile, SftpError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<TarDirectoryProfile, SftpError> {
        let mut profile = TarDirectoryProfile::default();
        let mut stack = VecDeque::from([(path, 0)]);
        while let Some((directory, depth)) = stack.pop_back() {
            if depth >= TAR_MAX_RECURSION_DEPTH {
                return Err(SftpError::TransferError(format!(
                    "directory profile recursion depth {TAR_MAX_RECURSION_DEPTH} reached at {}",
                    directory.display()
                )));
            }
            for entry in std::fs::read_dir(&directory).map_err(SftpError::IoError)? {
                let entry = entry.map_err(SftpError::IoError)?;
                let entry_path = entry.path();
                let metadata =
                    std::fs::symlink_metadata(&entry_path).map_err(SftpError::IoError)?;
                if metadata.file_type().is_symlink() {
                    continue;
                }
                if metadata.is_dir() {
                    stack.push_back((entry_path, depth + 1));
                } else if metadata.is_file() {
                    profile.record_file(&entry_path, metadata.len());
                }
            }
        }
        Ok(profile)
    })
    .await
    .map_err(|error| SftpError::TransferError(format!("directory scan panicked: {error}")))?
}

fn is_likely_compressed(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "7z" | "aac"
            | "avif"
            | "bz2"
            | "flac"
            | "gif"
            | "gz"
            | "heic"
            | "jpeg"
            | "jpg"
            | "m4a"
            | "mkv"
            | "mov"
            | "mp3"
            | "mp4"
            | "ogg"
            | "opus"
            | "pdf"
            | "png"
            | "rar"
            | "webm"
            | "webp"
            | "xz"
            | "zip"
            | "zst"
    )
}

async fn throttle(
    transferred: u64,
    started: Instant,
    transfer_id: &str,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
) -> Result<(), SftpError> {
    let Some(manager) = transfer_manager else {
        return Ok(());
    };
    let limit = manager.speed_limit_bps();
    if limit == 0 {
        return Ok(());
    }
    loop {
        manager.check_control(transfer_id).await?;
        let elapsed = started.elapsed().as_secs_f64();
        let expected = transferred as f64 / limit as f64;
        if expected <= elapsed {
            return Ok(());
        }
        tokio::time::sleep(
            Duration::from_secs_f64(expected - elapsed).min(TAR_CONTROL_POLL_INTERVAL),
        )
        .await;
    }
}

async fn send_progress(
    tx: &Option<mpsc::Sender<TransferProgress>>,
    id: &str,
    remote_path: &str,
    local_path: &str,
    direction: TransferDirection,
    total_bytes: u64,
    transferred_bytes: u64,
    started: Instant,
    state: TransferState,
) {
    let Some(tx) = tx else {
        return;
    };
    let elapsed = started.elapsed().as_secs_f64().max(0.001);
    let speed = (transferred_bytes as f64 / elapsed) as u64;
    let eta_seconds = if speed > 0 && total_bytes > transferred_bytes {
        Some(((total_bytes - transferred_bytes) as f64 / speed as f64) as u64)
    } else {
        Some(0)
    };
    let _ = tx
        .send(TransferProgress {
            id: id.to_string(),
            remote_path: remote_path.to_string(),
            local_path: local_path.to_string(),
            direction,
            state,
            total_bytes,
            transferred_bytes,
            speed,
            eta_seconds,
            error: None,
        })
        .await;
}

#[derive(Default)]
struct ExecExit {
    exit_code: Option<u32>,
    stderr: Vec<u8>,
    timed_out: bool,
}

async fn drain_channel_exit_with_timeout(
    channel: &mut russh::Channel<russh::client::Msg>,
    timeout: Duration,
) -> ExecExit {
    match tokio::time::timeout(timeout, drain_channel_exit_inner(channel)).await {
        Ok(exit) => exit,
        Err(_) => ExecExit {
            timed_out: true,
            ..ExecExit::default()
        },
    }
}

async fn drain_channel_exit_with_control(
    channel: &mut russh::Channel<russh::client::Msg>,
    transfer_id: &str,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
    idle_timeout: Duration,
) -> Result<ExecExit, SftpError> {
    let mut exit = ExecExit::default();
    loop {
        match wait_channel_message(channel, transfer_id, transfer_manager, idle_timeout).await? {
            Some(ChannelMsg::ExitStatus { exit_status }) => exit.exit_code = Some(exit_status),
            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                append_bounded(&mut exit.stderr, &data, TAR_MAX_STDERR_BYTES)
            }
            Some(ChannelMsg::Close) | None => return Ok(exit),
            Some(ChannelMsg::Eof) => {}
            _ => {}
        }
    }
}

async fn wait_channel_message(
    channel: &mut russh::Channel<russh::client::Msg>,
    transfer_id: &str,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
    idle_timeout: Duration,
) -> Result<Option<ChannelMsg>, SftpError> {
    let mut idle_started = Instant::now();
    loop {
        if let Some(manager) = transfer_manager {
            let was_paused = manager
                .get_control(transfer_id)
                .is_some_and(|control| control.is_paused());
            let control_check_started = Instant::now();
            manager.check_control(transfer_id).await?;
            if was_paused || control_check_started.elapsed() >= TAR_CONTROL_POLL_INTERVAL {
                idle_started = Instant::now();
            }
        }
        match tokio::time::timeout(TAR_CONTROL_POLL_INTERVAL, channel.wait()).await {
            Ok(message) => return Ok(message),
            Err(_) if idle_started.elapsed() >= idle_timeout => {
                return Err(SftpError::TransferError(
                    "Remote tar channel stopped making progress".to_string(),
                ));
            }
            Err(_) => {}
        }
    }
}

async fn drain_channel_exit_inner(channel: &mut russh::Channel<russh::client::Msg>) -> ExecExit {
    let mut exit = ExecExit::default();
    loop {
        match channel.wait().await {
            Some(ChannelMsg::ExitStatus { exit_status }) => exit.exit_code = Some(exit_status),
            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                append_bounded(&mut exit.stderr, &data, TAR_MAX_STDERR_BYTES)
            }
            Some(ChannelMsg::Close) | None => break,
            Some(ChannelMsg::Eof) => {}
            _ => {}
        }
    }
    exit
}

fn validate_exit(exit: ExecExit) -> Result<(), SftpError> {
    if exit.timed_out {
        return Err(SftpError::TransferError(
            "Remote tar did not finish before timeout".to_string(),
        ));
    }
    if exit.exit_code.is_some_and(|code| code != 0) {
        let stderr = String::from_utf8_lossy(&exit.stderr);
        return Err(SftpError::TransferError(format!(
            "Remote tar exited with code {}: {}",
            exit.exit_code.unwrap_or_default(),
            stderr.trim()
        )));
    }
    if exit.exit_code.is_none() {
        let stderr = String::from_utf8_lossy(&exit.stderr);
        return Err(SftpError::TransferError(if stderr.trim().is_empty() {
            "Remote tar closed without reporting an exit status".to_string()
        } else {
            format!(
                "Remote tar closed without reporting an exit status: {}",
                stderr.trim()
            )
        }));
    }
    Ok(())
}

fn append_bounded(target: &mut Vec<u8>, data: &[u8], limit: usize) {
    let remaining = limit.saturating_sub(target.len());
    target.extend_from_slice(&data[..data.len().min(remaining)]);
}

fn shell_escape(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone, Default)]
    struct CountingRejectedOpener {
        open_count: Arc<AtomicUsize>,
    }

    impl SftpExecChannelOpener for CountingRejectedOpener {
        fn open_exec_channel(
            &self,
        ) -> impl Future<Output = Result<russh::Channel<russh::client::Msg>, SftpError>> + Send
        {
            let open_count = self.open_count.clone();
            async move {
                open_count.fetch_add(1, Ordering::SeqCst);
                Err(SftpError::ChannelError(
                    "unexpected channel open".to_string(),
                ))
            }
        }
    }

    #[tokio::test]
    async fn tar_upload_preserves_task_level_cancellation_before_opening_a_channel() {
        let manager = Arc::new(SftpTransferManager::new());
        manager.register_for_node("tx-1", "node-a");
        assert!(manager.cancel("tx-1"));
        let opener = CountingRejectedOpener::default();

        let result = tar_upload_directory(
            &opener,
            "/path/does/not/need/to/exist",
            "/remote/path",
            "tx-1",
            None,
            Some(manager.clone()),
            TarTransferOptions {
                profile: TarDirectoryProfile::default(),
                compression: TarCompression::None,
            },
        )
        .await;

        assert!(matches!(result, Err(SftpError::TransferCancelled)));
        assert_eq!(opener.open_count.load(Ordering::SeqCst), 0);
        manager.unregister("tx-1");
        assert!(manager.get_control("tx-1").is_none());
    }

    #[test]
    fn tar_strategy_prefers_multiple_small_files_and_many_large_files() {
        let mut one_large_file = TarDirectoryProfile::default();
        one_large_file.record_file(Path::new("video.raw"), 32 * 1024 * 1024);
        assert!(!one_large_file.prefers_tar());

        let mut two_small_files = TarDirectoryProfile::default();
        two_small_files.record_file(Path::new("one.txt"), 1024);
        two_small_files.record_file(Path::new("two.txt"), 1024);
        assert!(two_small_files.prefers_tar());

        let mut many_large_files = TarDirectoryProfile::default();
        for index in 0..TAR_MIN_FILE_COUNT {
            many_large_files.record_file(Path::new(&format!("file-{index}.raw")), 32 * 1024 * 1024);
        }
        assert!(many_large_files.prefers_tar());
    }

    #[test]
    fn tar_strategy_disables_compression_for_already_compressed_payloads() {
        let mut compressed = TarDirectoryProfile::default();
        compressed.record_file(Path::new("archive.zip"), 9 * 1024);
        compressed.record_file(Path::new("notes.txt"), 1024);
        assert_eq!(
            compressed.recommended_compression(TarCompression::Zstd),
            TarCompression::None
        );

        let mut source = TarDirectoryProfile::default();
        source.record_file(Path::new("main.rs"), 10 * 1024);
        assert_eq!(
            source.recommended_compression(TarCompression::Zstd),
            TarCompression::Zstd
        );
    }

    #[test]
    fn tar_exit_requires_an_explicit_success_status() {
        assert!(validate_exit(ExecExit::default()).is_err());
        assert!(
            validate_exit(ExecExit {
                exit_code: Some(0),
                ..ExecExit::default()
            })
            .is_ok()
        );
    }

    #[test]
    fn tar_stderr_capture_is_bounded() {
        let mut stderr = Vec::new();
        append_bounded(&mut stderr, &[1; 128], 64);
        append_bounded(&mut stderr, &[2; 128], 64);
        assert_eq!(stderr.len(), 64);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn closing_the_stream_receiver_releases_a_blocked_encoder() {
        let (tx, mut rx) = mpsc::channel(1);
        let encoder = tokio::task::spawn_blocking(move || {
            let mut writer = ChunkWriter::new(tx);
            writer.write_all(&vec![0; TAR_STREAM_CHUNK_SIZE * 3])?;
            writer.flush()
        });

        assert!(rx.recv().await.is_some());
        drop(rx);
        let result = tokio::time::timeout(Duration::from_secs(1), encoder)
            .await
            .expect("closing the receiver must release blocking backpressure")
            .expect("encoder task must not panic");
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn tar_profile_and_encoder_skip_local_symlinks() {
        use std::io::Cursor;
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("inside.txt"), b"inside").unwrap();
        std::fs::write(outside.path().join("outside.txt"), b"outside").unwrap();
        symlink(outside.path(), root.path().join("external-link")).unwrap();

        let profile = profile_local_directory(root.path()).await.unwrap();
        assert_eq!(profile.file_count, 1);
        assert_eq!(profile.total_bytes, 6);

        let (tx, mut rx) = mpsc::channel(32);
        let root_path = root.path().to_string_lossy().to_string();
        let encoded_bytes = Arc::new(AtomicU64::new(0));
        let encoded_items = Arc::new(AtomicU64::new(0));
        let encoder = tokio::task::spawn_blocking({
            let encoded_bytes = encoded_bytes.clone();
            let encoded_items = encoded_items.clone();
            move || {
                tar_encode_directory(
                    &root_path,
                    tx,
                    TarCompression::None,
                    &encoded_bytes,
                    &encoded_items,
                )
            }
        });
        let mut archive_bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            archive_bytes.extend_from_slice(&chunk);
        }
        encoder.await.unwrap().unwrap();
        assert_eq!(encoded_items.load(Ordering::Relaxed), 1);

        let mut archive = tar::Archive::new(Cursor::new(archive_bytes.clone()));
        let paths = archive
            .entries()
            .unwrap()
            .map(|entry| entry.unwrap().path().unwrap().into_owned())
            .collect::<Vec<_>>();
        assert!(paths.iter().any(|path| path.ends_with("inside.txt")));
        assert!(!paths.iter().any(|path| path.ends_with("external-link")));
        assert!(!paths.iter().any(|path| path.ends_with("outside.txt")));

        let destination = tempfile::tempdir().unwrap();
        let (decode_tx, decode_rx) = mpsc::channel(1);
        decode_tx.send(Bytes::from(archive_bytes)).await.unwrap();
        drop(decode_tx);
        let decoded_bytes = Arc::new(AtomicU64::new(0));
        let decoder = tokio::task::spawn_blocking({
            let destination = destination.path().to_string_lossy().to_string();
            let decoded_bytes = decoded_bytes.clone();
            move || {
                tar_decode_directory(
                    &destination,
                    decode_rx,
                    TarCompression::None,
                    &decoded_bytes,
                )
            }
        });
        let decoded = decoder.await.unwrap().unwrap();
        assert_eq!(decoded.item_count, 1);
        assert_eq!(decoded.unpacked_bytes, 6);
        assert_eq!(
            std::fs::read(destination.path().join("inside.txt")).unwrap(),
            b"inside"
        );
    }
}
