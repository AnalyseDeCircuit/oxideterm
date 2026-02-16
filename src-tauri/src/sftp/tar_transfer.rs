//! Tar-on-the-fly streaming transfer for directories
//!
//! Instead of transferring thousands of small files via individual SFTP
//! open/write/close round-trips, this module packs a local directory into
//! a tar stream and pipes it through a single SSH exec channel to `tar -xf -`
//! on the remote side (upload), or runs `tar -cf -` on the remote and unpacks
//! locally (download).
//!
//! **Effect**: Reduces N×3 SFTP round-trips to a single sustained stream,
//! yielding 10–50× speedup for directories with many small files.
//!
//! **Cross-platform**: Linux/macOS always have `tar`. Windows 10 1803+
//! ships `tar.exe` (bsdtar). A capability probe (`tar --version`) is run
//! once per session and cached; if unavailable, callers fall back to the
//! regular SFTP path.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use russh::ChannelMsg;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::sftp::error::SftpError;
use crate::sftp::types::{TransferDirection, TransferProgress, TransferState};
use crate::ssh::HandleController;

// ============================================================================
// Compression support
// ============================================================================

/// Compression method for tar streaming transfers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TarCompression {
    /// No compression — plain tar stream
    None,
    /// zstd compression (best ratio/speed tradeoff, requires tar --zstd)
    Zstd,
    /// gzip compression (universal, requires tar -z)
    Gzip,
}

impl TarCompression {
    /// Get the tar flag for this compression method.
    fn tar_flag(&self) -> &'static str {
        match self {
            TarCompression::None => "",
            TarCompression::Zstd => " --zstd",
            TarCompression::Gzip => " -z",
        }
    }
}

// ============================================================================
// Capability probe
// ============================================================================

/// Check whether the remote host has `tar` available.
///
/// Runs `tar --version` via exec channel and returns `true` if exit code == 0.
/// This is intentionally cheap (~1 round-trip) and the result should be cached
/// per SSH session.
pub async fn probe_tar_support(controller: &HandleController) -> bool {
    match probe_tar_inner(controller).await {
        Ok(available) => {
            debug!("Remote tar probe result: {}", available);
            available
        }
        Err(e) => {
            warn!("Remote tar probe failed: {}", e);
            false
        }
    }
}

async fn probe_tar_inner(controller: &HandleController) -> Result<bool, SftpError> {
    let mut channel = controller
        .open_session_channel()
        .await
        .map_err(|e| SftpError::ChannelError(format!("Failed to open probe channel: {}", e)))?;

    channel
        .exec(true, "tar --version")
        .await
        .map_err(|e| SftpError::ChannelError(format!("Failed to exec tar probe: {}", e)))?;

    let mut exit_code: Option<u32> = None;

    // Drain the channel (we don't need the output)
    let drain = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match channel.wait().await {
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    exit_code = Some(exit_status);
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
    })
    .await;

    if drain.is_err() {
        let _ = channel.close().await;
        return Ok(false);
    }

    Ok(exit_code == Some(0))
}

/// Probe which compression methods the remote `tar` supports.
///
/// Tests zstd first (best ratio/speed), then gzip, falling back to None.
/// Returns the best available compression. Result should be cached per session.
pub async fn probe_tar_compression(controller: &HandleController) -> TarCompression {
    // Test zstd: tar --zstd -cf /dev/null /dev/null
    if probe_exec_exit0(
        controller,
        "tar --zstd -cf /dev/null /dev/null 2>/dev/null",
    )
    .await
    {
        info!("Remote tar supports zstd compression");
        return TarCompression::Zstd;
    }

    // Test gzip: tar -zcf /dev/null /dev/null
    if probe_exec_exit0(controller, "tar -zcf /dev/null /dev/null 2>/dev/null").await {
        info!("Remote tar supports gzip compression");
        return TarCompression::Gzip;
    }

    info!("Remote tar has no compression support, using plain tar");
    TarCompression::None
}

/// Run a command and check if it exits with code 0.
async fn probe_exec_exit0(controller: &HandleController, cmd: &str) -> bool {
    let channel_result = controller.open_session_channel().await;
    let mut channel = match channel_result {
        Ok(ch) => ch,
        Err(_) => return false,
    };

    if channel.exec(true, cmd).await.is_err() {
        let _ = channel.close().await;
        return false;
    }

    let mut exit_code: Option<u32> = None;
    let drain = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match channel.wait().await {
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    exit_code = Some(exit_status);
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
    })
    .await;

    if drain.is_err() {
        let _ = channel.close().await;
        return false;
    }

    exit_code == Some(0)
}

// ============================================================================
// Upload: local → tar stream → SSH exec → remote untar
// ============================================================================

/// Stream-upload a local directory to the remote host via `tar`.
///
/// 1. Scans `local_path` to calculate total size (for progress reporting).
/// 2. Opens an SSH exec channel running `tar [-z|--zstd] -xf - -C <remote_path>`.
/// 3. Builds a tar archive on-the-fly from `local_path`, writing chunks
///    directly into the SSH channel.
/// 4. Reports progress on each chunk written.
///
/// # Compression
/// Pass `TarCompression::Zstd` or `Gzip` to compress the stream. The remote
/// tar must support the corresponding flag (use `probe_tar_compression` first).
///
/// # Cancel
/// If `cancel_flag` is set, the transfer aborts and the channel is closed.
pub async fn tar_upload_directory(
    controller: &HandleController,
    local_path: &str,
    remote_path: &str,
    transfer_id: &str,
    progress_tx: Option<mpsc::Sender<TransferProgress>>,
    cancel_flag: Option<Arc<AtomicBool>>,
    compression: Option<TarCompression>,
    speed_limit_bps: Option<Arc<std::sync::atomic::AtomicUsize>>,
) -> Result<u64, SftpError> {
    let local = Path::new(local_path);
    if !local.is_dir() {
        return Err(SftpError::DirectoryNotFound(local_path.into()));
    }

    let comp = compression.unwrap_or(TarCompression::None);

    // Phase 1: scan total size
    let total_bytes = dir_total_size(local).await?;
    info!(
        "tar upload: {} → {} ({} bytes total, compression={:?})",
        local_path, remote_path, total_bytes, comp
    );

    // Phase 2: open tar -xf channel
    // NOTE: remote directory creation is handled by caller via SFTP mkdir,
    // so we avoid shell-specific `mkdir -p` here for better cross-platform compatibility.

    let mut channel = controller
        .open_session_channel()
        .await
        .map_err(|e| SftpError::ChannelError(format!("Failed to open tar channel: {}", e)))?;

    // tar [-z|--zstd] -xf - -C <remote>  :  read tar from stdin, extract into remote_path
    let cmd = format!("tar{} -xf - -C {}", comp.tar_flag(), shell_escape(remote_path));
    debug!("tar upload exec: {}", cmd);

    channel
        .exec(true, cmd)
        .await
        .map_err(|e| SftpError::ChannelError(format!("Failed to exec tar: {}", e)))?;

    // Phase 3: build tar archive and stream into channel
    let start = Instant::now();
    let mut bytes_sent: u64 = 0;
    let mut last_progress = Instant::now();

    // Build the tar archive into an in-memory buffer, then send in chunks.
    // We use a synchronous tar::Builder writing into a Vec, then flush
    // that Vec into the SSH channel. This is done in a streaming fashion
    // using a spsc approach: a blocking task produces tar data, the async
    // task consumes and sends it over SSH.
    let local_path_owned = local_path.to_string();
    let (data_tx, mut data_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(32);

    // Spawn blocking tar builder in a thread (with optional compression)
    let tar_handle = tokio::task::spawn_blocking(move || -> Result<(), SftpError> {
        tar_encode_directory(&local_path_owned, data_tx, comp)
    });

    // Consume tar chunks and write to SSH channel
    while let Some(chunk) = data_rx.recv().await {
        // Check cancellation
        if let Some(ref flag) = cancel_flag {
            if flag.load(Ordering::Relaxed) {
                let _ = channel.close().await;
                return Err(SftpError::TransferCancelled);
            }
        }

        channel
            .data(&chunk[..])
            .await
            .map_err(|e| SftpError::ChannelError(format!("Failed to write tar data: {}", e)))?;

        bytes_sent += chunk.len() as u64;

        // Speed limit throttle (token-bucket style)
        if let Some(ref limit) = speed_limit_bps {
            let bps = limit.load(Ordering::Relaxed);
            if bps > 0 {
                let elapsed = start.elapsed().as_secs_f64();
                let expected_secs = bytes_sent as f64 / bps as f64;
                if expected_secs > elapsed {
                    tokio::time::sleep(std::time::Duration::from_secs_f64(expected_secs - elapsed)).await;
                }
            }
        }

        // Throttle progress to 200ms
        if last_progress.elapsed().as_millis() >= 200 {
            if let Some(ref tx) = progress_tx {
                let elapsed = start.elapsed().as_secs_f64().max(0.001);
                let speed = (bytes_sent as f64 / elapsed) as u64;
                let remaining = total_bytes.saturating_sub(bytes_sent);
                let eta = if speed > 0 {
                    Some((remaining as f64 / speed as f64) as u64)
                } else {
                    None
                };
                let _ = tx
                    .try_send(TransferProgress {
                        id: transfer_id.to_string(),
                        remote_path: remote_path.to_string(),
                        local_path: local_path.to_string(),
                        direction: TransferDirection::Upload,
                        state: TransferState::InProgress,
                        total_bytes,
                        transferred_bytes: bytes_sent,
                        speed,
                        eta_seconds: eta,
                        error: None,
                    });
                last_progress = Instant::now();
            }
        }
    }

    // Wait for tar builder to finish
    let tar_result = tar_handle
        .await
        .map_err(|e| SftpError::TransferError(format!("tar builder thread panicked: {}", e)))?;
    tar_result?;

    // Signal EOF to the remote tar process
    channel
        .eof()
        .await
        .map_err(|e| SftpError::ChannelError(format!("Failed to send EOF: {}", e)))?;

    // Wait for remote tar to finish and check exit status
    let exit_code = drain_channel_exit(&mut channel).await;
    let _ = channel.close().await;

    if let Some(code) = exit_code {
        if code != 0 {
            return Err(SftpError::TransferError(format!(
                "Remote tar exited with code {}",
                code
            )));
        }
    }

    // Final progress
    if let Some(ref tx) = progress_tx {
        let elapsed = start.elapsed().as_secs_f64().max(0.001);
        let _ = tx
            .try_send(TransferProgress {
                id: transfer_id.to_string(),
                remote_path: remote_path.to_string(),
                local_path: local_path.to_string(),
                direction: TransferDirection::Upload,
                state: TransferState::Completed,
                total_bytes,
                transferred_bytes: bytes_sent,
                speed: (bytes_sent as f64 / elapsed) as u64,
                eta_seconds: Some(0),
                error: None,
            });
    }

    info!(
        "tar upload complete: {} bytes in {:.1}s",
        bytes_sent,
        start.elapsed().as_secs_f64()
    );

    Ok(bytes_sent)
}

// ============================================================================
// Download: remote tar → SSH channel → local untar
// ============================================================================

/// Stream-download a remote directory via `tar` to the local filesystem.
///
/// 1. Opens an SSH exec channel running `tar -cf - -C <remote_path> .`
/// 2. Reads the tar stream from the channel and unpacks locally.
/// 3. Reports progress based on bytes received.
pub async fn tar_download_directory(
    controller: &HandleController,
    remote_path: &str,
    local_path: &str,
    transfer_id: &str,
    progress_tx: Option<mpsc::Sender<TransferProgress>>,
    cancel_flag: Option<Arc<AtomicBool>>,
    compression: Option<TarCompression>,
    speed_limit_bps: Option<Arc<std::sync::atomic::AtomicUsize>>,
) -> Result<u64, SftpError> {
    let local = Path::new(local_path);
    let comp = compression.unwrap_or(TarCompression::None);

    // Ensure local directory exists
    tokio::fs::create_dir_all(local)
        .await
        .map_err(|e| SftpError::IoError(e))?;

    info!(
        "tar download: {} → {} (compression={:?})",
        remote_path, local_path, comp
    );

    let mut channel = controller
        .open_session_channel()
        .await
        .map_err(|e| SftpError::ChannelError(format!("Failed to open tar channel: {}", e)))?;

    // tar [-z|--zstd] -cf - -C <remote> .  :  create tar from remote_path, write to stdout
    let cmd = format!(
        "tar{} -cf - -C {} .",
        comp.tar_flag(),
        shell_escape(remote_path)
    );
    debug!("tar download exec: {}", cmd);

    channel
        .exec(true, cmd)
        .await
        .map_err(|e| SftpError::ChannelError(format!("Failed to exec tar: {}", e)))?;

    // Stream data from channel into the tar decoder
    let start = Instant::now();
    let mut bytes_received: u64 = 0;
    let mut last_progress = Instant::now();
    let mut exit_code: Option<u32> = None;

    // We collect tar data into an async pipe: channel → data_tx → blocking tar decoder.
    // Use tokio mpsc to avoid blocking the async runtime thread.
    let (data_tx, data_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let local_path_owned = local_path.to_string();

    // Spawn blocking tar decoder (with optional decompression)
    let decode_handle = tokio::task::spawn_blocking(move || -> Result<(), SftpError> {
        tar_decode_directory(&local_path_owned, data_rx, comp)
    });

    // Read from SSH channel and feed into decoder
    let mut stderr_buf = Vec::new();
    loop {
        // Check cancellation
        if let Some(ref flag) = cancel_flag {
            if flag.load(Ordering::Relaxed) {
                let _ = channel.close().await;
                drop(data_tx);
                let _ = decode_handle.await;
                return Err(SftpError::TransferCancelled);
            }
        }

        match channel.wait().await {
            Some(ChannelMsg::Data { data }) => {
                bytes_received += data.len() as u64;

                if data_tx.send(data.to_vec()).await.is_err() {
                    // Decoder died
                    break;
                }

                // Speed limit throttle (token-bucket style)
                if let Some(ref limit) = speed_limit_bps {
                    let bps = limit.load(Ordering::Relaxed);
                    if bps > 0 {
                        let elapsed = start.elapsed().as_secs_f64();
                        let expected_secs = bytes_received as f64 / bps as f64;
                        if expected_secs > elapsed {
                            tokio::time::sleep(std::time::Duration::from_secs_f64(expected_secs - elapsed)).await;
                        }
                    }
                }

                // Throttle progress to 200ms
                if last_progress.elapsed().as_millis() >= 200 {
                    if let Some(ref tx) = progress_tx {
                        let elapsed = start.elapsed().as_secs_f64().max(0.001);
                        let speed = (bytes_received as f64 / elapsed) as u64;
                        let _ = tx
                            .try_send(TransferProgress {
                                id: transfer_id.to_string(),
                                remote_path: remote_path.to_string(),
                                local_path: local_path.to_string(),
                                direction: TransferDirection::Download,
                                state: TransferState::InProgress,
                                // For tar download we don't know total size upfront;
                                // set total_bytes = 0 so the frontend shows a
                                // streaming/indeterminate progress indicator.
                                total_bytes: 0,
                                transferred_bytes: bytes_received,
                                speed,
                                eta_seconds: None,
                                error: None,
                            });
                        last_progress = Instant::now();
                    }
                }
            }
            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                stderr_buf.extend_from_slice(&data);
            }
            Some(ChannelMsg::ExitStatus { exit_status }) => {
                exit_code = Some(exit_status);
            }
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => break,
            None => break,
            _ => {}
        }
    }

    // Close the data channel to signal decoder EOF
    drop(data_tx);

    // Wait for decoder to finish
    let decode_result = decode_handle
        .await
        .map_err(|e| SftpError::TransferError(format!("tar decoder thread panicked: {}", e)))?;
    decode_result?;

    let _ = channel.close().await;

    // Check remote tar exit status
    if let Some(code) = exit_code {
        if code != 0 {
            let stderr_str = String::from_utf8_lossy(&stderr_buf);
            return Err(SftpError::TransferError(format!(
                "Remote tar exited with code {}: {}",
                code,
                stderr_str.trim()
            )));
        }
    }

    // Final progress
    if let Some(ref tx) = progress_tx {
        let elapsed = start.elapsed().as_secs_f64().max(0.001);
        let _ = tx
            .try_send(TransferProgress {
                id: transfer_id.to_string(),
                remote_path: remote_path.to_string(),
                local_path: local_path.to_string(),
                direction: TransferDirection::Download,
                state: TransferState::Completed,
                total_bytes: bytes_received,
                transferred_bytes: bytes_received,
                speed: (bytes_received as f64 / elapsed) as u64,
                eta_seconds: Some(0),
                error: None,
            });
    }

    info!(
        "tar download complete: {} bytes in {:.1}s",
        bytes_received,
        start.elapsed().as_secs_f64()
    );

    Ok(bytes_received)
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Synchronously encode a directory into tar (optionally compressed),
/// sending chunks over an mpsc channel.
///
/// Runs on a blocking thread. Uses `tar::Builder` with a custom `Write` impl
/// that sends data in ~256KB chunks through the channel.
/// When `compression` is `Gzip` or `Zstd`, the tar stream is piped through
/// the corresponding compressor so the remote `tar -z/-zstd -xf -` can decode.
fn tar_encode_directory(
    local_path: &str,
    data_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    compression: TarCompression,
) -> Result<(), SftpError> {
    use std::io::Write;

    struct ChunkWriter {
        tx: tokio::sync::mpsc::Sender<Vec<u8>>,
        buf: Vec<u8>,
    }

    const CHUNK_SIZE: usize = 256 * 1024; // 256 KB

    impl Write for ChunkWriter {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.buf.extend_from_slice(data);
            while self.buf.len() >= CHUNK_SIZE {
                let chunk: Vec<u8> = self.buf.drain(..CHUNK_SIZE).collect();
                self.tx.blocking_send(chunk).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::BrokenPipe, "channel closed")
                })?;
            }
            Ok(data.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            if !self.buf.is_empty() {
                let chunk = std::mem::take(&mut self.buf);
                self.tx.blocking_send(chunk).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::BrokenPipe, "channel closed")
                })?;
            }
            Ok(())
        }
    }

    impl Drop for ChunkWriter {
        fn drop(&mut self) {
            // Flush remaining data on drop
            if !self.buf.is_empty() {
                let chunk = std::mem::take(&mut self.buf);
                let _ = self.tx.blocking_send(chunk);
            }
        }
    }

    let chunk_writer = ChunkWriter {
        tx: data_tx,
        buf: Vec::with_capacity(CHUNK_SIZE),
    };

    // Helper: build tar into a generic writer then finish+flush.
    fn build_tar<W: Write>(mut writer: W, local_path: &str) -> Result<W, SftpError> {
        let mut builder = tar::Builder::new(&mut writer);
        builder.follow_symlinks(true);
        builder.mode(tar::HeaderMode::Deterministic);
        let base = Path::new(local_path);
        builder
            .append_dir_all(".", base)
            .map_err(|e| SftpError::IoError(e))?;
        builder
            .into_inner()
            .map_err(|e| SftpError::IoError(e))?;
        Ok(writer)
    }

    match compression {
        TarCompression::None => {
            let mut w = build_tar(chunk_writer, local_path)?;
            w.flush().map_err(SftpError::IoError)?;
        }
        TarCompression::Gzip => {
            let gz = flate2::write::GzEncoder::new(chunk_writer, flate2::Compression::fast());
            let gz = build_tar(gz, local_path)?;
            gz.finish().map_err(SftpError::IoError)?;
        }
        TarCompression::Zstd => {
            let zst = zstd::Encoder::new(chunk_writer, 3)
                .map_err(SftpError::IoError)?;
            let zst = build_tar(zst, local_path)?;
            zst.finish().map_err(SftpError::IoError)?;
        }
    }

    Ok(())
}

/// Synchronously decode a tar stream from a sync channel into a local directory.
///
/// Runs on a blocking thread. Reads chunks from `data_rx`, pipes them through
/// an optional decompressor and then a `tar::Archive`, and extracts all entries.
fn tar_decode_directory(
    local_path: &str,
    data_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    compression: TarCompression,
) -> Result<(), SftpError> {
    use std::io::Read;

    /// Adapter: tokio mpsc::Receiver<Vec<u8>> → Read
    struct ChannelReader {
        rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
        buf: Vec<u8>,
        pos: usize,
    }

    impl Read for ChannelReader {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            while self.pos >= self.buf.len() {
                match self.rx.blocking_recv() {
                    Some(chunk) => {
                        self.buf = chunk;
                        self.pos = 0;
                    }
                    None => return Ok(0), // channel closed = EOF
                }
            }

            let available = &self.buf[self.pos..];
            let n = available.len().min(out.len());
            out[..n].copy_from_slice(&available[..n]);
            self.pos += n;
            Ok(n)
        }
    }

    let raw_reader = ChannelReader {
        rx: data_rx,
        buf: Vec::new(),
        pos: 0,
    };

    // Helper: unpack a tar archive from any Read impl.
    fn unpack_tar<R: Read>(reader: R, local_path: &str) -> Result<(), SftpError> {
        let mut archive = tar::Archive::new(reader);
        archive.set_preserve_permissions(true);
        archive
            .unpack(local_path)
            .map_err(|e| SftpError::IoError(e))?;
        Ok(())
    }

    match compression {
        TarCompression::None => unpack_tar(raw_reader, local_path)?,
        TarCompression::Gzip => {
            let gz = flate2::read::GzDecoder::new(raw_reader);
            unpack_tar(gz, local_path)?;
        }
        TarCompression::Zstd => {
            let zst = zstd::Decoder::new(raw_reader)
                .map_err(SftpError::IoError)?;
            unpack_tar(zst, local_path)?;
        }
    }

    Ok(())
}

/// Calculate total size of a local directory tree (async, follows symlinks).
async fn dir_total_size(path: &Path) -> Result<u64, SftpError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut total: u64 = 0;
        for entry in walkdir::WalkDir::new(&path).follow_links(true) {
            let entry = entry.map_err(|e| {
                SftpError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e))
            })?;
            if entry.file_type().is_file() {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        Ok(total)
    })
    .await
    .map_err(|e| SftpError::TransferError(format!("size scan panicked: {}", e)))?
}

/// Drain a channel until EOF/Close, returning the exit code if received.
async fn drain_channel_exit(channel: &mut russh::Channel<russh::client::Msg>) -> Option<u32> {
    let mut exit_code = None;

    let drain = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            match channel.wait().await {
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    exit_code = Some(exit_status);
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
    })
    .await;

    if drain.is_err() {
        warn!("drain_channel_exit timed out");
    }

    exit_code
}

/// Shell-escape a path using double quotes.
///
/// Double-quoted form is accepted by POSIX shells and Windows cmd/powershell.
fn shell_escape(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\\\""))
}
