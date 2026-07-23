// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Bounded legacy SCP control-record and channel transport.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use bytes::{Bytes, BytesMut};
use russh::{Channel, ChannelMsg, client};
use tokio::{fs::File, io::AsyncWriteExt, sync::mpsc};

use super::paths::validate_received_name;
use super::{append_bounded, check_control, send_progress, throttle};
use crate::{
    SftpError, SftpExecChannelOpener, SftpTransferManager, TransferDirection, TransferProgress,
    TransferState,
};

const SCP_MAX_CONTROL_LINE_BYTES: usize = 16 * 1024;
const SCP_MAX_STDERR_BYTES: usize = 64 * 1024;
const SCP_EXIT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Eq, PartialEq)]
pub(super) enum ScpRecord {
    File { mode: u32, size: u64, name: String },
    Directory { mode: u32, name: String },
    EndDirectory,
    Times,
}

pub(super) struct ScpChannel {
    channel: Channel<client::Msg>,
    buffered: BytesMut,
    stderr: Vec<u8>,
    exit_status: Option<u32>,
    closed: bool,
}

impl ScpChannel {
    fn new(channel: Channel<client::Msg>) -> Self {
        Self {
            channel,
            buffered: BytesMut::new(),
            stderr: Vec::new(),
            exit_status: None,
            closed: false,
        }
    }

    pub(super) async fn send_ack(&self) -> Result<(), SftpError> {
        self.send_data(&[0]).await
    }

    pub(super) async fn send_data(&self, data: &[u8]) -> Result<(), SftpError> {
        self.channel
            .data_bytes(Bytes::copy_from_slice(data))
            .await
            .map_err(|error| SftpError::ChannelError(format!("Failed to write SCP data: {error}")))
    }

    pub(super) async fn send_control_line(&self, line: &str) -> Result<(), SftpError> {
        if line.len() > SCP_MAX_CONTROL_LINE_BYTES || line.contains(['\n', '\r', '\0']) {
            return Err(SftpError::ProtocolError(
                "Invalid SCP control line".to_string(),
            ));
        }
        let mut data = Vec::with_capacity(line.len() + 1);
        data.extend_from_slice(line.as_bytes());
        data.push(b'\n');
        self.send_data(&data).await
    }

    pub(super) async fn read_response(&mut self) -> Result<(), SftpError> {
        match self.read_byte().await? {
            0 => Ok(()),
            1 | 2 => {
                let message = self.read_line().await?;
                Err(SftpError::ProtocolError(format!(
                    "Remote SCP rejected the transfer: {message}"
                )))
            }
            byte => Err(SftpError::ProtocolError(format!(
                "Remote SCP sent invalid response byte {byte}"
            ))),
        }
    }

    pub(super) async fn expect_sender_status(&mut self) -> Result<(), SftpError> {
        self.read_response().await
    }

    pub(super) async fn read_next_record(&mut self) -> Result<ScpRecord, SftpError> {
        let kind = self.read_byte().await?;
        match kind {
            1 | 2 => {
                let message = self.read_line().await?;
                Err(SftpError::ProtocolError(format!(
                    "Remote SCP reported an error: {message}"
                )))
            }
            b'C' | b'D' => {
                let line = self.read_line().await?;
                parse_file_or_directory_record(kind, &line)
            }
            b'E' => {
                let line = self.read_line().await?;
                if !line.is_empty() {
                    return Err(SftpError::ProtocolError(
                        "Malformed SCP end-directory record".to_string(),
                    ));
                }
                Ok(ScpRecord::EndDirectory)
            }
            b'T' => {
                let line = self.read_line().await?;
                validate_time_record(&line)?;
                Ok(ScpRecord::Times)
            }
            byte => Err(SftpError::ProtocolError(format!(
                "Remote SCP sent unknown control record {byte}"
            ))),
        }
    }

    async fn read_byte(&mut self) -> Result<u8, SftpError> {
        self.ensure_buffered().await?;
        Ok(self.buffered.split_to(1)[0])
    }

    async fn read_line(&mut self) -> Result<String, SftpError> {
        loop {
            if let Some(line) = take_control_line(&mut self.buffered)? {
                return Ok(line);
            }
            self.receive_message().await?;
        }
    }

    async fn ensure_buffered(&mut self) -> Result<(), SftpError> {
        while self.buffered.is_empty() {
            self.receive_message().await?;
        }
        Ok(())
    }

    async fn receive_message(&mut self) -> Result<(), SftpError> {
        match self.channel.wait().await {
            Some(ChannelMsg::Data { data }) => {
                self.buffered.extend_from_slice(&data);
                Ok(())
            }
            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                append_bounded(&mut self.stderr, &data, SCP_MAX_STDERR_BYTES);
                Ok(())
            }
            Some(ChannelMsg::ExitStatus { exit_status }) => {
                self.exit_status = Some(exit_status);
                Ok(())
            }
            Some(ChannelMsg::Eof) => Ok(()),
            Some(ChannelMsg::Close) | None => {
                self.closed = true;
                Err(SftpError::ProtocolError(
                    "Remote SCP closed the channel before completing the protocol".to_string(),
                ))
            }
            _ => Ok(()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn copy_exact_to_file(
        &mut self,
        file: &mut File,
        mut remaining: u64,
        progress_offset: u64,
        progress_total: u64,
        transfer_id: &str,
        remote_path: &str,
        local_path: &str,
        started: Instant,
        progress_tx: &Option<mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> Result<(), SftpError> {
        let mut transferred = 0u64;
        let mut last_progress = Instant::now();
        while remaining > 0 {
            check_control(transfer_manager, transfer_id).await?;
            self.ensure_buffered().await?;
            let take = remaining.min(self.buffered.len() as u64) as usize;
            let data = self.buffered.split_to(take);
            file.write_all(&data).await.map_err(SftpError::IoError)?;
            remaining -= take as u64;
            transferred += take as u64;
            let aggregate_transferred = progress_offset.saturating_add(transferred);
            throttle(aggregate_transferred, started, transfer_manager).await;
            if last_progress.elapsed() >= Duration::from_millis(200) {
                // Recursive SCP discovers file sizes while walking the stream, so
                // the aggregate total grows as each file header arrives.
                send_progress(
                    progress_tx,
                    transfer_id,
                    remote_path,
                    local_path,
                    TransferDirection::Download,
                    progress_total,
                    aggregate_transferred,
                    started,
                    TransferState::InProgress,
                )
                .await;
                last_progress = Instant::now();
            }
        }
        Ok(())
    }

    pub(super) async fn send_eof(&self) -> Result<(), SftpError> {
        self.channel.eof().await.map_err(|error| {
            SftpError::ChannelError(format!("Failed to finish SCP input: {error}"))
        })
    }

    pub(super) async fn finish(&mut self) -> Result<(), SftpError> {
        let drain = async {
            while !self.closed {
                match self.channel.wait().await {
                    Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                        append_bounded(&mut self.stderr, &data, SCP_MAX_STDERR_BYTES);
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        self.exit_status = Some(exit_status);
                    }
                    Some(ChannelMsg::Close) | None => self.closed = true,
                    _ => {}
                }
            }
        };
        tokio::time::timeout(SCP_EXIT_TIMEOUT, drain)
            .await
            .map_err(|_| SftpError::ChannelError("Remote SCP exit timed out".to_string()))?;
        let _ = self.channel.close().await;
        if self.exit_status == Some(0) {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&self.stderr).trim().to_string();
        Err(SftpError::ChannelError(if stderr.is_empty() {
            format!("Remote SCP exited with status {:?}", self.exit_status)
        } else {
            format!(
                "Remote SCP exited with status {:?}: {stderr}",
                self.exit_status
            )
        }))
    }
}

pub(super) fn take_control_line(buffer: &mut BytesMut) -> Result<Option<String>, SftpError> {
    let Some(index) = buffer.iter().position(|byte| *byte == b'\n') else {
        if buffer.len() > SCP_MAX_CONTROL_LINE_BYTES {
            return Err(SftpError::ProtocolError(
                "SCP control line exceeds the safety limit".to_string(),
            ));
        }
        return Ok(None);
    };
    if index > SCP_MAX_CONTROL_LINE_BYTES {
        return Err(SftpError::ProtocolError(
            "SCP control line exceeds the safety limit".to_string(),
        ));
    }
    let mut line = buffer.split_to(index + 1);
    line.truncate(index);
    String::from_utf8(line.to_vec())
        .map(Some)
        .map_err(|_| SftpError::ProtocolError("SCP control line is not valid UTF-8".to_string()))
}

pub(super) async fn open_scp_channel<O>(opener: &O, command: &str) -> Result<ScpChannel, SftpError>
where
    O: SftpExecChannelOpener,
{
    let channel = opener.open_exec_channel().await?;
    channel
        .exec(true, command)
        .await
        .map_err(|error| SftpError::ChannelError(format!("Failed to start remote SCP: {error}")))?;
    Ok(ScpChannel::new(channel))
}

pub(super) fn parse_file_or_directory_record(kind: u8, line: &str) -> Result<ScpRecord, SftpError> {
    let mut fields = line.splitn(3, ' ');
    let mode = fields
        .next()
        .filter(|value| value.len() == 4 && value.bytes().all(|byte| (b'0'..=b'7').contains(&byte)))
        .ok_or_else(|| SftpError::ProtocolError("Invalid SCP mode".to_string()))?;
    let size = fields
        .next()
        .ok_or_else(|| SftpError::ProtocolError("Missing SCP size".to_string()))?
        .parse::<u64>()
        .map_err(|_| SftpError::ProtocolError("Invalid SCP size".to_string()))?;
    let name = fields
        .next()
        .ok_or_else(|| SftpError::ProtocolError("Missing SCP file name".to_string()))?
        .to_string();
    validate_received_name(&name)?;
    let mode = u32::from_str_radix(mode, 8)
        .map_err(|_| SftpError::ProtocolError("Invalid SCP mode".to_string()))?;
    match kind {
        b'C' => Ok(ScpRecord::File { mode, size, name }),
        b'D' if size == 0 => Ok(ScpRecord::Directory { mode, name }),
        b'D' => Err(SftpError::ProtocolError(
            "SCP directory record has a non-zero size".to_string(),
        )),
        _ => Err(SftpError::ProtocolError(
            "Unknown SCP record type".to_string(),
        )),
    }
}

pub(super) fn validate_time_record(line: &str) -> Result<(), SftpError> {
    let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
    if fields.len() != 4 || fields.iter().any(|field| field.parse::<u64>().is_err()) {
        return Err(SftpError::ProtocolError(
            "Malformed SCP time record".to_string(),
        ));
    }
    Ok(())
}
