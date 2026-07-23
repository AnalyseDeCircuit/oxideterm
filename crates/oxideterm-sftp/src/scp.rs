// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Legacy SCP protocol transfers over a node-owned SSH exec channel.

mod channel;
mod paths;

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use channel::{ScpChannel, ScpRecord, open_scp_channel};
#[cfg(test)]
use channel::{parse_file_or_directory_record, take_control_line, validate_time_record};
use paths::{
    cleanup_remote_directory, contained_child_path, directory_total_size, ensure_entry_limit,
    local_directory_temporary_path, local_temporary_path, remote_backup_directory_path,
    remote_directory_replace_command, remote_temporary_directory_path, remote_temporary_path,
    replace_local_directory, replace_local_path, safe_local_file_name, validate_received_name,
    validate_remote_path,
};
use russh::ChannelMsg;
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
};

use crate::{
    SftpError, SftpExecChannelOpener, SftpTransferGuard, SftpTransferManager, TransferDirection,
    TransferProgress, TransferState, remote_parent_path, shell_quote,
};

const SCP_STREAM_CHUNK_SIZE: usize = 256 * 1024;
const SCP_EXEC_EXIT_TIMEOUT: Duration = Duration::from_secs(15);
const SCP_MAX_DIRECTORY_DEPTH: usize = 128;
const SCP_MAX_DIRECTORY_ENTRIES: u64 = 1_000_000;

/// SCP availability for one live SSH connection generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScpCapabilities {
    pub supports_scp: bool,
    pub supports_recursive: bool,
}

impl ScpCapabilities {
    pub const fn unsupported() -> Self {
        Self {
            supports_scp: false,
            supports_recursive: false,
        }
    }
}

/// Aggregate result returned by one SCP transfer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScpTransferResult {
    pub bytes: u64,
    pub items: u64,
}

/// Detects the POSIX remote `scp` executable without opening another SSH transport.
pub async fn probe_scp_support<O>(opener: &O) -> bool
where
    O: SftpExecChannelOpener,
{
    run_exec_exit0(opener, "command -v scp >/dev/null 2>&1").await
}

/// Probes SCP capabilities once per connection generation through the transfer manager.
pub async fn probe_scp_capabilities<O>(opener: &O) -> ScpCapabilities
where
    O: SftpExecChannelOpener,
{
    if !probe_scp_support(opener).await {
        return ScpCapabilities::unsupported();
    }
    ScpCapabilities {
        supports_scp: true,
        supports_recursive: true,
    }
}

/// Uploads one file with the legacy SCP sink protocol.
pub async fn scp_upload_file<O>(
    opener: &O,
    local_path: &str,
    remote_path: &str,
    transfer_id: &str,
    progress_tx: Option<mpsc::Sender<TransferProgress>>,
    transfer_manager: Option<Arc<SftpTransferManager>>,
) -> Result<ScpTransferResult, SftpError>
where
    O: SftpExecChannelOpener,
{
    let _control = transfer_manager
        .as_ref()
        .map(|manager| manager.register(transfer_id));
    let _control_guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
    check_control(&transfer_manager, transfer_id).await?;

    let local = Path::new(local_path);
    let metadata = fs::symlink_metadata(local)
        .await
        .map_err(SftpError::IoError)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(SftpError::InvalidPath(
            "SCP file upload requires a regular, non-symlink file".to_string(),
        ));
    }
    let file_name = safe_local_file_name(local)?;
    validate_remote_path(remote_path)?;
    let remote_temp = remote_temporary_path(remote_path)?;

    let result = upload_file_to_remote_path(
        opener,
        local,
        &file_name,
        &remote_temp,
        remote_path,
        metadata.len(),
        transfer_id,
        progress_tx,
        transfer_manager.clone(),
    )
    .await;

    match result {
        Ok(result) => {
            let finalize = format!(
                "mv -f -- {} {}",
                shell_quote(&remote_temp),
                shell_quote(remote_path)
            );
            if let Err(error) = run_required_exec(opener, &finalize, "finalize SCP upload").await {
                // A completed protocol stream can still fail during the final
                // atomic rename, so remove only the generated sibling.
                let cleanup = format!("rm -f -- {}", shell_quote(&remote_temp));
                let _ = run_exec_exit0(opener, &cleanup).await;
                return Err(error);
            }
            Ok(result)
        }
        Err(error) => {
            // Cleanup is best effort and targets only the generated sibling path.
            let cleanup = format!("rm -f -- {}", shell_quote(&remote_temp));
            let _ = run_exec_exit0(opener, &cleanup).await;
            Err(error)
        }
    }
}

/// Downloads one file with the legacy SCP source protocol.
pub async fn scp_download_file<O>(
    opener: &O,
    remote_path: &str,
    local_path: &str,
    transfer_id: &str,
    progress_tx: Option<mpsc::Sender<TransferProgress>>,
    transfer_manager: Option<Arc<SftpTransferManager>>,
) -> Result<ScpTransferResult, SftpError>
where
    O: SftpExecChannelOpener,
{
    let _control = transfer_manager
        .as_ref()
        .map(|manager| manager.register(transfer_id));
    let _control_guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
    check_control(&transfer_manager, transfer_id).await?;
    validate_remote_path(remote_path)?;

    let local = Path::new(local_path);
    let local_name = safe_local_file_name(local)?;
    let local_temp = local_temporary_path(local)?;
    if let Some(parent) = local
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .await
            .map_err(SftpError::IoError)?;
    }

    let result = download_file_to_local_path(
        opener,
        remote_path,
        &local_temp,
        &local_name,
        transfer_id,
        progress_tx,
        transfer_manager,
    )
    .await;
    match result {
        Ok(result) => {
            replace_local_path(&local_temp, local).await?;
            Ok(result)
        }
        Err(error) => {
            let _ = fs::remove_file(&local_temp).await;
            Err(error)
        }
    }
}

/// Uploads a directory recursively with SCP control records.
pub async fn scp_upload_directory<O>(
    opener: &O,
    local_path: &str,
    remote_path: &str,
    transfer_id: &str,
    progress_tx: Option<mpsc::Sender<TransferProgress>>,
    transfer_manager: Option<Arc<SftpTransferManager>>,
) -> Result<ScpTransferResult, SftpError>
where
    O: SftpExecChannelOpener,
{
    let _control = transfer_manager
        .as_ref()
        .map(|manager| manager.register(transfer_id));
    let _control_guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
    check_control(&transfer_manager, transfer_id).await?;
    validate_remote_path(remote_path)?;

    let local = Path::new(local_path);
    let metadata = fs::symlink_metadata(local)
        .await
        .map_err(SftpError::IoError)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(SftpError::InvalidPath(
            "SCP directory upload requires a real directory".to_string(),
        ));
    }
    let remote_temp = remote_temporary_directory_path(remote_path)?;
    let remote_backup = remote_backup_directory_path(remote_path)?;
    let root_name = safe_local_file_name(Path::new(&remote_temp))?;
    let remote_parent = remote_parent_path(&remote_temp);
    let total_bytes = directory_total_size(local).await?;
    let transfer = async {
        let mut stream = open_scp_channel(
            opener,
            &format!("scp -tr -- {}", shell_quote(&remote_parent)),
        )
        .await?;
        stream.read_response().await?;

        let started = Instant::now();
        let mut progress = ScpTransferResult::default();
        upload_directory_tree(
            &mut stream,
            local,
            &root_name,
            0,
            total_bytes,
            &mut progress,
            started,
            transfer_id,
            local_path,
            remote_path,
            &progress_tx,
            &transfer_manager,
        )
        .await?;
        stream.send_eof().await?;
        stream.finish().await?;
        Ok::<_, SftpError>((progress, started))
    }
    .await;

    let (progress, started) = match transfer {
        Ok(result) => result,
        Err(error) => {
            cleanup_remote_directory(opener, &remote_temp).await;
            return Err(error);
        }
    };
    let finalize = remote_directory_replace_command(&remote_temp, remote_path, &remote_backup);
    if let Err(error) = run_required_exec(opener, &finalize, "finalize SCP directory upload").await
    {
        cleanup_remote_directory(opener, &remote_temp).await;
        return Err(error);
    }
    send_progress(
        &progress_tx,
        transfer_id,
        remote_path,
        local_path,
        TransferDirection::Upload,
        total_bytes,
        total_bytes,
        started,
        TransferState::Completed,
    )
    .await;
    Ok(progress)
}

/// Downloads a directory recursively while containing every remote name under the target root.
pub async fn scp_download_directory<O>(
    opener: &O,
    remote_path: &str,
    local_path: &str,
    transfer_id: &str,
    progress_tx: Option<mpsc::Sender<TransferProgress>>,
    transfer_manager: Option<Arc<SftpTransferManager>>,
) -> Result<ScpTransferResult, SftpError>
where
    O: SftpExecChannelOpener,
{
    let _control = transfer_manager
        .as_ref()
        .map(|manager| manager.register(transfer_id));
    let _control_guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
    check_control(&transfer_manager, transfer_id).await?;
    validate_remote_path(remote_path)?;

    let local = Path::new(local_path);
    let local_temp = local_directory_temporary_path(local)?;
    if let Some(parent) = local
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .await
            .map_err(SftpError::IoError)?;
    }
    let _ = fs::remove_dir_all(&local_temp).await;
    fs::create_dir_all(&local_temp)
        .await
        .map_err(SftpError::IoError)?;

    let result = async {
        let command = format!("scp -fr -- {}", shell_quote(remote_path));
        let mut stream = open_scp_channel(opener, &command).await?;
        stream.send_ack().await?;
        let started = Instant::now();
        let mut progress = ScpTransferResult::default();
        receive_directory_root(
            &mut stream,
            &local_temp,
            0,
            &mut progress,
            started,
            transfer_id,
            local_path,
            remote_path,
            &progress_tx,
            &transfer_manager,
        )
        .await?;
        stream.finish().await?;
        Ok::<_, SftpError>(progress)
    }
    .await;

    match result {
        Ok(result) => {
            replace_local_directory(&local_temp, local).await?;
            Ok(result)
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&local_temp).await;
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn upload_file_to_remote_path<O>(
    opener: &O,
    local_path: &Path,
    file_name: &str,
    remote_temp: &str,
    final_remote_path: &str,
    total_bytes: u64,
    transfer_id: &str,
    progress_tx: Option<mpsc::Sender<TransferProgress>>,
    transfer_manager: Option<Arc<SftpTransferManager>>,
) -> Result<ScpTransferResult, SftpError>
where
    O: SftpExecChannelOpener,
{
    let command = format!("scp -t -- {}", shell_quote(remote_temp));
    let mut stream = open_scp_channel(opener, &command).await?;
    stream.read_response().await?;
    stream
        .send_control_line(&format!("C0644 {total_bytes} {file_name}"))
        .await?;
    stream.read_response().await?;

    let mut file = File::open(local_path).await.map_err(SftpError::IoError)?;
    let mut buffer = vec![0u8; SCP_STREAM_CHUNK_SIZE];
    let started = Instant::now();
    let mut transferred = 0u64;
    let mut last_progress = Instant::now();
    loop {
        check_control(&transfer_manager, transfer_id).await?;
        let read = file.read(&mut buffer).await.map_err(SftpError::IoError)?;
        if read == 0 {
            break;
        }
        stream.send_data(&buffer[..read]).await?;
        transferred = transferred.saturating_add(read as u64);
        throttle(transferred, started, &transfer_manager).await;
        if last_progress.elapsed() >= Duration::from_millis(200) {
            send_progress(
                &progress_tx,
                transfer_id,
                final_remote_path,
                &local_path.to_string_lossy(),
                TransferDirection::Upload,
                total_bytes,
                transferred,
                started,
                TransferState::InProgress,
            )
            .await;
            last_progress = Instant::now();
        }
    }
    stream.send_data(&[0]).await?;
    stream.read_response().await?;
    stream.send_eof().await?;
    stream.finish().await?;
    send_progress(
        &progress_tx,
        transfer_id,
        final_remote_path,
        &local_path.to_string_lossy(),
        TransferDirection::Upload,
        total_bytes,
        total_bytes,
        started,
        TransferState::Completed,
    )
    .await;
    Ok(ScpTransferResult {
        bytes: total_bytes,
        items: 1,
    })
}

#[allow(clippy::too_many_arguments)]
async fn download_file_to_local_path<O>(
    opener: &O,
    remote_path: &str,
    local_temp: &Path,
    expected_name: &str,
    transfer_id: &str,
    progress_tx: Option<mpsc::Sender<TransferProgress>>,
    transfer_manager: Option<Arc<SftpTransferManager>>,
) -> Result<ScpTransferResult, SftpError>
where
    O: SftpExecChannelOpener,
{
    let command = format!("scp -f -- {}", shell_quote(remote_path));
    let mut stream = open_scp_channel(opener, &command).await?;
    stream.send_ack().await?;
    let header = stream.read_next_record().await?;
    let ScpRecord::File { size, name, .. } = header else {
        return Err(SftpError::ProtocolError(
            "SCP source did not send a file record".to_string(),
        ));
    };
    validate_received_name(&name)?;
    let remote_name = Path::new(remote_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(expected_name);
    if name != remote_name {
        return Err(SftpError::ProtocolError(format!(
            "SCP source sent unexpected file name: {name}"
        )));
    }
    stream.send_ack().await?;

    let mut file = File::create(local_temp).await.map_err(SftpError::IoError)?;
    let started = Instant::now();
    stream
        .copy_exact_to_file(
            &mut file,
            size,
            0,
            size,
            transfer_id,
            remote_path,
            &local_temp.to_string_lossy(),
            started,
            &progress_tx,
            &transfer_manager,
        )
        .await?;
    file.flush().await.map_err(SftpError::IoError)?;
    file.sync_all().await.map_err(SftpError::IoError)?;
    stream.expect_sender_status().await?;
    stream.send_ack().await?;
    send_progress(
        &progress_tx,
        transfer_id,
        remote_path,
        &local_temp.to_string_lossy(),
        TransferDirection::Download,
        size,
        size,
        started,
        TransferState::Completed,
    )
    .await;
    Ok(ScpTransferResult {
        bytes: size,
        items: 1,
    })
}

#[allow(clippy::too_many_arguments)]
async fn upload_directory_tree(
    stream: &mut ScpChannel,
    local_path: &Path,
    remote_name: &str,
    depth: usize,
    total_bytes: u64,
    progress: &mut ScpTransferResult,
    started: Instant,
    transfer_id: &str,
    local_root: &str,
    remote_root: &str,
    progress_tx: &Option<mpsc::Sender<TransferProgress>>,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
) -> Result<(), SftpError> {
    if depth > SCP_MAX_DIRECTORY_DEPTH {
        return Err(SftpError::ProtocolError(
            "SCP directory nesting limit exceeded".to_string(),
        ));
    }
    validate_received_name(remote_name)?;
    stream
        .send_control_line(&format!("D0755 0 {remote_name}"))
        .await?;
    stream.read_response().await?;
    progress.items = progress.items.saturating_add(1);
    ensure_entry_limit(progress.items)?;

    let mut entries = fs::read_dir(local_path).await.map_err(SftpError::IoError)?;
    while let Some(entry) = entries.next_entry().await.map_err(SftpError::IoError)? {
        check_control(transfer_manager, transfer_id).await?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .await
            .map_err(SftpError::IoError)?;
        if metadata.file_type().is_symlink() {
            return Err(SftpError::InvalidPath(format!(
                "SCP recursive upload does not follow symlink: {}",
                path.display()
            )));
        }
        let name = safe_local_file_name(&path)?;
        if metadata.is_dir() {
            Box::pin(upload_directory_tree(
                stream,
                &path,
                &name,
                depth + 1,
                total_bytes,
                progress,
                started,
                transfer_id,
                local_root,
                remote_root,
                progress_tx,
                transfer_manager,
            ))
            .await?;
        } else if metadata.is_file() {
            stream
                .send_control_line(&format!("C0644 {} {name}", metadata.len()))
                .await?;
            stream.read_response().await?;
            let mut file = File::open(&path).await.map_err(SftpError::IoError)?;
            let mut buffer = vec![0u8; SCP_STREAM_CHUNK_SIZE];
            loop {
                check_control(transfer_manager, transfer_id).await?;
                let read = file.read(&mut buffer).await.map_err(SftpError::IoError)?;
                if read == 0 {
                    break;
                }
                stream.send_data(&buffer[..read]).await?;
                progress.bytes = progress.bytes.saturating_add(read as u64);
                throttle(progress.bytes, started, transfer_manager).await;
                send_progress(
                    progress_tx,
                    transfer_id,
                    remote_root,
                    local_root,
                    TransferDirection::Upload,
                    total_bytes,
                    progress.bytes,
                    started,
                    TransferState::InProgress,
                )
                .await;
            }
            stream.send_data(&[0]).await?;
            stream.read_response().await?;
            progress.items = progress.items.saturating_add(1);
            ensure_entry_limit(progress.items)?;
        }
    }
    stream.send_control_line("E").await?;
    stream.read_response().await
}

#[allow(clippy::too_many_arguments)]
async fn receive_directory_root(
    stream: &mut ScpChannel,
    local_root: &Path,
    depth: usize,
    progress: &mut ScpTransferResult,
    started: Instant,
    transfer_id: &str,
    local_path: &str,
    remote_path: &str,
    progress_tx: &Option<mpsc::Sender<TransferProgress>>,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
) -> Result<(), SftpError> {
    let record = stream.read_next_record().await?;
    let ScpRecord::Directory { name, .. } = record else {
        return Err(SftpError::ProtocolError(
            "SCP source did not send a directory record".to_string(),
        ));
    };
    validate_received_name(&name)?;
    stream.send_ack().await?;
    receive_directory_contents(
        stream,
        local_root,
        depth,
        progress,
        started,
        transfer_id,
        local_path,
        remote_path,
        progress_tx,
        transfer_manager,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn receive_directory_contents(
    stream: &mut ScpChannel,
    local_directory: &Path,
    depth: usize,
    progress: &mut ScpTransferResult,
    started: Instant,
    transfer_id: &str,
    local_path: &str,
    remote_path: &str,
    progress_tx: &Option<mpsc::Sender<TransferProgress>>,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
) -> Result<(), SftpError> {
    if depth > SCP_MAX_DIRECTORY_DEPTH {
        return Err(SftpError::ProtocolError(
            "SCP directory nesting limit exceeded".to_string(),
        ));
    }
    progress.items = progress.items.saturating_add(1);
    ensure_entry_limit(progress.items)?;
    loop {
        check_control(transfer_manager, transfer_id).await?;
        match stream.read_next_record().await? {
            ScpRecord::Times => {
                stream.send_ack().await?;
            }
            ScpRecord::EndDirectory => {
                stream.send_ack().await?;
                return Ok(());
            }
            ScpRecord::Directory { name, .. } => {
                validate_received_name(&name)?;
                let child = contained_child_path(local_directory, &name)?;
                fs::create_dir(&child).await.map_err(SftpError::IoError)?;
                stream.send_ack().await?;
                Box::pin(receive_directory_contents(
                    stream,
                    &child,
                    depth + 1,
                    progress,
                    started,
                    transfer_id,
                    local_path,
                    remote_path,
                    progress_tx,
                    transfer_manager,
                ))
                .await?;
            }
            ScpRecord::File { size, name, .. } => {
                validate_received_name(&name)?;
                let child = contained_child_path(local_directory, &name)?;
                let mut file = File::create(&child).await.map_err(SftpError::IoError)?;
                stream.send_ack().await?;
                let before = progress.bytes;
                stream
                    .copy_exact_to_file(
                        &mut file,
                        size,
                        before,
                        before.saturating_add(size),
                        transfer_id,
                        remote_path,
                        local_path,
                        started,
                        progress_tx,
                        transfer_manager,
                    )
                    .await?;
                file.flush().await.map_err(SftpError::IoError)?;
                file.sync_all().await.map_err(SftpError::IoError)?;
                stream.expect_sender_status().await?;
                stream.send_ack().await?;
                progress.bytes = before.saturating_add(size);
                progress.items = progress.items.saturating_add(1);
                ensure_entry_limit(progress.items)?;
            }
        }
    }
}

async fn check_control(
    manager: &Option<Arc<SftpTransferManager>>,
    transfer_id: &str,
) -> Result<(), SftpError> {
    if let Some(manager) = manager {
        manager.check_control(transfer_id).await?;
    }
    Ok(())
}

async fn throttle(transferred: u64, started: Instant, manager: &Option<Arc<SftpTransferManager>>) {
    let Some(manager) = manager else {
        return;
    };
    let limit = manager.speed_limit_bps();
    if limit == 0 {
        return;
    }
    let expected = transferred as f64 / limit as f64;
    let elapsed = started.elapsed().as_secs_f64();
    if expected > elapsed {
        tokio::time::sleep(Duration::from_secs_f64(expected - elapsed)).await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn send_progress(
    tx: &Option<mpsc::Sender<TransferProgress>>,
    transfer_id: &str,
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
    let eta_seconds = (speed > 0 && total_bytes > transferred_bytes)
        .then(|| (total_bytes - transferred_bytes) / speed)
        .or(Some(0));
    let _ = tx
        .send(TransferProgress {
            id: transfer_id.to_string(),
            remote_path: remote_path.to_string(),
            local_path: local_path.to_string(),
            direction,
            total_bytes,
            transferred_bytes,
            speed,
            eta_seconds,
            state,
            error: None,
        })
        .await;
}

async fn run_required_exec<O>(opener: &O, command: &str, operation: &str) -> Result<(), SftpError>
where
    O: SftpExecChannelOpener,
{
    if run_exec_exit0(opener, command).await {
        Ok(())
    } else {
        Err(SftpError::ChannelError(format!(
            "Remote command failed while attempting to {operation}"
        )))
    }
}

async fn run_exec_exit0<O>(opener: &O, command: &str) -> bool
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
    let result = tokio::time::timeout(SCP_EXEC_EXIT_TIMEOUT, async {
        let mut exit_status = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::ExitStatus {
                    exit_status: status,
                } => exit_status = Some(status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        exit_status == Some(0)
    })
    .await
    .unwrap_or(false);
    let _ = channel.close().await;
    result
}

fn append_bounded(target: &mut Vec<u8>, data: &[u8], limit: usize) {
    let remaining = limit.saturating_sub(target.len());
    target.extend_from_slice(&data[..data.len().min(remaining)]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn parses_file_directory_end_and_time_records() {
        assert_eq!(
            parse_file_or_directory_record(b'C', "0644 12 notes.txt").unwrap(),
            ScpRecord::File {
                mode: 0o644,
                size: 12,
                name: "notes.txt".to_string(),
            }
        );
        assert_eq!(
            parse_file_or_directory_record(b'D', "0755 0 src").unwrap(),
            ScpRecord::Directory {
                mode: 0o755,
                name: "src".to_string(),
            }
        );
        assert!(validate_time_record("1710000000 0 1710000001 0").is_ok());
    }

    #[test]
    fn rejects_traversal_and_control_characters_in_remote_names() {
        for name in ["", ".", "..", "../escape", "a/b", "a\\b", "a\nb", "a\0b"] {
            assert!(validate_received_name(name).is_err(), "{name:?}");
        }
    }

    #[test]
    fn rejects_malformed_modes_sizes_and_directory_lengths() {
        assert!(parse_file_or_directory_record(b'C', "0999 1 file").is_err());
        assert!(parse_file_or_directory_record(b'C', "0644 huge file").is_err());
        assert!(parse_file_or_directory_record(b'D', "0755 1 dir").is_err());
    }

    #[test]
    fn control_line_decoder_waits_for_split_packets_and_preserves_following_data() {
        let mut buffer = BytesMut::from(&b"0644 12 no"[..]);
        assert_eq!(take_control_line(&mut buffer).unwrap(), None);
        buffer.extend_from_slice(b"tes.txt\npayload");

        assert_eq!(
            take_control_line(&mut buffer).unwrap(),
            Some("0644 12 notes.txt".to_string())
        );
        assert_eq!(&buffer[..], b"payload");
    }

    #[test]
    fn builds_generated_temporary_paths_as_siblings() {
        let remote = remote_temporary_path("/srv/data/report.txt").unwrap();
        assert!(remote.starts_with("/srv/data/.report.txt.oxideterm-"));
        assert!(remote.ends_with(".part"));

        let remote_directory = remote_temporary_directory_path("/srv/data/project").unwrap();
        assert!(remote_directory.starts_with("/srv/data/.project.oxideterm-"));
        assert!(remote_directory.ends_with(".part-dir"));

        let local = local_temporary_path(Path::new("/tmp/report.txt")).unwrap();
        assert_eq!(local.parent(), Some(Path::new("/tmp")));
        assert!(
            local
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".report.txt.oxideterm-")
        );
    }

    #[test]
    fn legacy_progress_records_default_to_sftp() {
        let json = r#"{
            "transfer_id":"tx",
            "transfer_type":"Upload",
            "source_path":"/tmp/a",
            "destination_path":"/tmp/b",
            "transferred_bytes":0,
            "total_bytes":1,
            "status":"Failed",
            "last_updated":"2026-01-01T00:00:00Z",
            "session_id":"session",
            "error":null
        }"#;
        let progress: crate::StoredTransferProgress = serde_json::from_str(json).unwrap();
        assert_eq!(progress.protocol, crate::TransferProtocol::Sftp);
    }
}
