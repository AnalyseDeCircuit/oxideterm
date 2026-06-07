// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    future::Future,
    io::{Read, Write},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use bytes::Bytes;
use russh::ChannelMsg;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::{SftpError, SftpTransferManager, TransferDirection, TransferProgress, TransferState};

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

pub async fn tar_upload_directory<O>(
    opener: &O,
    local_path: &str,
    remote_path: &str,
    transfer_id: &str,
    progress_tx: Option<mpsc::Sender<TransferProgress>>,
    transfer_manager: Option<Arc<SftpTransferManager>>,
    compression: Option<TarCompression>,
) -> Result<u64, SftpError>
where
    O: SftpExecChannelOpener,
{
    let local = Path::new(local_path);
    if !local.is_dir() {
        return Err(SftpError::DirectoryNotFound(local_path.to_string()));
    }
    let compression = compression.unwrap_or(TarCompression::None);
    let total_bytes = dir_total_size(local).await?;

    let mut channel = opener.open_exec_channel().await?;
    let cmd = format!(
        "tar{} -xf - -C {}",
        compression.tar_flag(),
        shell_escape(remote_path)
    );
    debug!("tar upload exec: {cmd}");
    channel
        .exec(true, cmd)
        .await
        .map_err(|error| SftpError::ChannelError(format!("Failed to exec tar: {error}")))?;

    let (data_tx, mut data_rx) = mpsc::channel::<Vec<u8>>(32);
    // tar::Builder is synchronous. Keep it on a blocking thread and bridge it
    // to the async SSH channel with bounded chunks, matching the Tauri pipeline.
    let tar_handle = tokio::task::spawn_blocking({
        let local_path = local_path.to_string();
        move || tar_encode_directory(&local_path, data_tx, compression)
    });

    let start = Instant::now();
    let mut sent = 0u64;
    let mut last_progress = Instant::now();
    while let Some(chunk) = data_rx.recv().await {
        if let Some(manager) = &transfer_manager {
            if let Err(error) = manager.check_control(transfer_id).await {
                let _ = channel.close().await;
                let _ = tar_handle.await;
                return Err(error);
            }
        }
        channel.data(&chunk[..]).await.map_err(|error| {
            SftpError::ChannelError(format!("Failed to write tar data: {error}"))
        })?;
        sent += chunk.len() as u64;
        throttle(sent, start, &transfer_manager).await;
        if last_progress.elapsed().as_millis() >= 200 {
            send_progress(
                &progress_tx,
                transfer_id,
                remote_path,
                local_path,
                TransferDirection::Upload,
                total_bytes,
                sent.min(total_bytes),
                start,
                TransferState::InProgress,
            )
            .await;
            last_progress = Instant::now();
        }
    }
    tar_handle
        .await
        .map_err(|error| SftpError::TransferError(format!("tar builder panicked: {error}")))??;
    channel
        .eof()
        .await
        .map_err(|error| SftpError::ChannelError(format!("Failed to send EOF: {error}")))?;
    validate_exit(drain_channel_exit(&mut channel).await)?;
    let _ = channel.close().await;
    send_progress(
        &progress_tx,
        transfer_id,
        remote_path,
        local_path,
        TransferDirection::Upload,
        total_bytes,
        total_bytes,
        start,
        TransferState::Completed,
    )
    .await;
    Ok(total_bytes)
}

pub async fn tar_download_directory<O>(
    opener: &O,
    remote_path: &str,
    local_path: &str,
    transfer_id: &str,
    progress_tx: Option<mpsc::Sender<TransferProgress>>,
    transfer_manager: Option<Arc<SftpTransferManager>>,
    compression: Option<TarCompression>,
) -> Result<u64, SftpError>
where
    O: SftpExecChannelOpener,
{
    tokio::fs::create_dir_all(local_path)
        .await
        .map_err(SftpError::IoError)?;
    let compression = compression.unwrap_or(TarCompression::None);
    let mut channel = opener.open_exec_channel().await?;
    let cmd = format!(
        "tar{} -cf - -C {} .",
        compression.tar_flag(),
        shell_escape(remote_path)
    );
    debug!("tar download exec: {cmd}");
    channel
        .exec(true, cmd)
        .await
        .map_err(|error| SftpError::ChannelError(format!("Failed to exec tar: {error}")))?;

    let start = Instant::now();
    let (data_tx, data_rx) = mpsc::channel::<Bytes>(64);
    let decode_handle = tokio::task::spawn_blocking({
        let local_path = local_path.to_string();
        move || tar_decode_directory(&local_path, data_rx, compression)
    });

    let mut stderr = Vec::new();
    let mut exit_code = None;
    let mut received = 0u64;
    let mut last_progress = Instant::now();
    loop {
        if let Some(manager) = &transfer_manager {
            if let Err(error) = manager.check_control(transfer_id).await {
                let _ = channel.close().await;
                drop(data_tx);
                let _ = decode_handle.await;
                return Err(error);
            }
        }
        match channel.wait().await {
            Some(ChannelMsg::Data { data: chunk }) => {
                received += chunk.len() as u64;
                if data_tx.send(chunk).await.is_err() {
                    break;
                }
                throttle(received, start, &transfer_manager).await;
                if last_progress.elapsed().as_millis() >= 200 {
                    send_progress(
                        &progress_tx,
                        transfer_id,
                        remote_path,
                        local_path,
                        TransferDirection::Download,
                        0,
                        received,
                        start,
                        TransferState::InProgress,
                    )
                    .await;
                    last_progress = Instant::now();
                }
            }
            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => stderr.extend_from_slice(&data),
            Some(ChannelMsg::ExitStatus { exit_status }) => exit_code = Some(exit_status),
            Some(ChannelMsg::Eof) => {}
            Some(ChannelMsg::Close) | None => break,
            _ => {}
        }
    }
    drop(data_tx);
    decode_handle
        .await
        .map_err(|error| SftpError::TransferError(format!("tar decoder panicked: {error}")))??;
    let _ = channel.close().await;
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
        received,
        received,
        start,
        TransferState::Completed,
    )
    .await;
    Ok(received)
}

async fn probe_exec_exit0<O>(opener: &O, command: &str) -> bool
where
    O: SftpExecChannelOpener,
{
    let Ok(mut channel) = opener.open_exec_channel().await else {
        return false;
    };
    if channel.exec(true, command).await.is_err() {
        let _ = channel.close().await;
        return false;
    }
    let exit = drain_channel_exit_with_timeout(&mut channel, Duration::from_secs(10)).await;
    let _ = channel.close().await;
    !exit.timed_out && exit.exit_code == Some(0)
}

const TAR_STREAM_CHUNK_SIZE: usize = 256 * 1024;

struct ChunkWriter {
    tx: mpsc::Sender<Vec<u8>>,
    buffer: Vec<u8>,
}

impl ChunkWriter {
    fn new(tx: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            tx,
            buffer: Vec::with_capacity(TAR_STREAM_CHUNK_SIZE),
        }
    }
}

impl Write for ChunkWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(data);
        while self.buffer.len() >= TAR_STREAM_CHUNK_SIZE {
            let chunk = self.buffer.drain(..TAR_STREAM_CHUNK_SIZE).collect();
            self.tx.blocking_send(chunk).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::BrokenPipe, "tar stream closed")
            })?;
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.buffer.is_empty() {
            let chunk = std::mem::take(&mut self.buffer);
            self.tx.blocking_send(chunk).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::BrokenPipe, "tar stream closed")
            })?;
        }
        Ok(())
    }
}

impl Drop for ChunkWriter {
    fn drop(&mut self) {
        if !self.buffer.is_empty() {
            let chunk = std::mem::take(&mut self.buffer);
            let _ = self.tx.blocking_send(chunk);
        }
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
    data_tx: mpsc::Sender<Vec<u8>>,
    compression: TarCompression,
) -> Result<(), SftpError> {
    fn append_tar<W: Write>(writer: W, local_path: &str) -> Result<W, SftpError> {
        let mut builder = tar::Builder::new(writer);
        builder.follow_symlinks(true);
        builder.mode(tar::HeaderMode::Complete);
        builder
            .append_dir_all(".", Path::new(local_path))
            .map_err(SftpError::IoError)?;
        builder.into_inner().map_err(SftpError::IoError)
    }

    let writer = ChunkWriter::new(data_tx);
    match compression {
        TarCompression::None => {
            let mut writer = append_tar(writer, local_path)?;
            writer.flush().map_err(SftpError::IoError)?;
        }
        TarCompression::Gzip => {
            let encoder = flate2::write::GzEncoder::new(writer, flate2::Compression::fast());
            let encoder = append_tar(encoder, local_path)?;
            let mut writer = encoder.finish().map_err(SftpError::IoError)?;
            writer.flush().map_err(SftpError::IoError)?;
        }
        TarCompression::Zstd => {
            let encoder = zstd::Encoder::new(writer, 3).map_err(SftpError::IoError)?;
            let encoder = append_tar(encoder, local_path)?;
            let mut writer = encoder.finish().map_err(SftpError::IoError)?;
            writer.flush().map_err(SftpError::IoError)?;
        }
    }
    Ok(())
}

fn tar_decode_directory(
    local_path: &str,
    data_rx: mpsc::Receiver<Bytes>,
    compression: TarCompression,
) -> Result<(), SftpError> {
    fn unpack_tar<R: Read>(reader: R, local_path: &str) -> Result<(), SftpError> {
        let mut archive = tar::Archive::new(reader);
        archive.set_preserve_permissions(true);
        archive.unpack(local_path).map_err(SftpError::IoError)
    }

    let reader = ChannelReader::new(data_rx);
    match compression {
        TarCompression::None => unpack_tar(reader, local_path),
        TarCompression::Gzip => {
            let decoder = flate2::read::GzDecoder::new(reader);
            unpack_tar(decoder, local_path)
        }
        TarCompression::Zstd => {
            let decoder = zstd::Decoder::new(reader).map_err(SftpError::IoError)?;
            unpack_tar(decoder, local_path)
        }
    }
}

async fn dir_total_size(path: &Path) -> Result<u64, SftpError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<u64, SftpError> {
        fn walk(path: &Path, total: &mut u64) -> Result<(), SftpError> {
            for entry in std::fs::read_dir(path).map_err(SftpError::IoError)? {
                let entry = entry.map_err(SftpError::IoError)?;
                let metadata = entry.metadata().map_err(SftpError::IoError)?;
                if metadata.is_dir() {
                    walk(&entry.path(), total)?;
                } else if metadata.is_file() {
                    *total += metadata.len();
                }
            }
            Ok(())
        }
        let mut total = 0;
        walk(&path, &mut total)?;
        Ok(total)
    })
    .await
    .map_err(|error| SftpError::TransferError(format!("directory scan panicked: {error}")))?
}

async fn throttle(
    transferred: u64,
    started: Instant,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
) {
    let Some(manager) = transfer_manager else {
        return;
    };
    let limit = manager.speed_limit_bps();
    if limit == 0 {
        return;
    }
    let elapsed = started.elapsed().as_secs_f64();
    let expected = transferred as f64 / limit as f64;
    if expected > elapsed {
        tokio::time::sleep(Duration::from_secs_f64(expected - elapsed)).await;
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

async fn drain_channel_exit(channel: &mut russh::Channel<russh::client::Msg>) -> ExecExit {
    drain_channel_exit_inner(channel, None).await
}

async fn drain_channel_exit_with_timeout(
    channel: &mut russh::Channel<russh::client::Msg>,
    timeout: Duration,
) -> ExecExit {
    match tokio::time::timeout(timeout, drain_channel_exit_inner(channel, Some(timeout))).await {
        Ok(exit) => exit,
        Err(_) => ExecExit {
            timed_out: true,
            ..ExecExit::default()
        },
    }
}

async fn drain_channel_exit_inner(
    channel: &mut russh::Channel<russh::client::Msg>,
    _timeout: Option<Duration>,
) -> ExecExit {
    let mut exit = ExecExit::default();
    loop {
        match channel.wait().await {
            Some(ChannelMsg::ExitStatus { exit_status }) => exit.exit_code = Some(exit_status),
            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => exit.stderr.extend_from_slice(&data),
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
    if exit.exit_code.is_none() && !exit.stderr.is_empty() {
        let stderr = String::from_utf8_lossy(&exit.stderr);
        warn!(
            "Remote tar wrote stderr without exit status: {}",
            stderr.trim()
        );
    }
    Ok(())
}

fn shell_escape(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}
