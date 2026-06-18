// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use oxideterm_modem_transfer::xymodem_transfer::{
    XmodemBlockMode, YmodemSendStreamEntry, receive_xmodem, receive_ymodem, send_xmodem,
    send_ymodem_stream,
};
use oxideterm_modem_transfer::zmodem_transfer::{
    ZmodemSendStreamEntry, receive_zmodem, send_zmodem_stream,
};
use oxideterm_modem_transfer::{
    DetectedModemProtocol, ModemError, ModemTransfer, ModemTransferDirection, ModemTransferError,
};
use oxideterm_terminal::TerminalModemTransferRequest;

#[derive(Clone)]
pub(crate) enum ModemPromptSelection {
    UploadFiles(Vec<String>),
    DownloadRoot(String),
    Cancelled,
}

pub(crate) struct ModemWorkerJob {
    pub transfer: ModemTransfer,
    pub request: TerminalModemTransferRequest,
    pub selection: ModemPromptSelection,
}

pub(crate) enum ModemWorkerEvent {
    Progress(ModemWorkerProgress),
    Completed,
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug)]
pub(crate) struct ModemWorkerProgress {
    pub file_name: Option<String>,
    pub transferred_bytes: u64,
    pub total_bytes: Option<u64>,
}

pub(crate) fn run_modem_worker_job(
    mut job: ModemWorkerJob,
    event_tx: std::sync::mpsc::Sender<ModemWorkerEvent>,
) {
    let result = run_modem_worker_job_inner(&mut job, &event_tx);
    let event = match result {
        Ok(()) => ModemWorkerEvent::Completed,
        Err(ModemWorkerError::Cancelled) => ModemWorkerEvent::Cancelled,
        Err(ModemWorkerError::Failed(message)) => ModemWorkerEvent::Failed(message),
    };
    let _ = event_tx.send(event);
}

enum ModemWorkerError {
    Cancelled,
    Failed(String),
}

fn run_modem_worker_job_inner(
    job: &mut ModemWorkerJob,
    event_tx: &std::sync::mpsc::Sender<ModemWorkerEvent>,
) -> Result<(), ModemWorkerError> {
    let direction = job.request.direction;
    let selection = job.selection.clone();
    match (direction, selection) {
        (_, ModemPromptSelection::Cancelled) => {
            job.transfer.stop();
            Err(ModemWorkerError::Cancelled)
        }
        (ModemTransferDirection::Download, ModemPromptSelection::DownloadRoot(root)) => {
            run_download(job, Path::new(&root), event_tx)
        }
        (ModemTransferDirection::Upload, ModemPromptSelection::UploadFiles(paths)) => {
            run_upload(job, &paths, event_tx)
        }
        _ => Err(ModemWorkerError::Failed(
            "Invalid file selection for modem transfer".to_string(),
        )),
    }
}

fn run_download(
    job: &mut ModemWorkerJob,
    root: &Path,
    event_tx: &std::sync::mpsc::Sender<ModemWorkerEvent>,
) -> Result<(), ModemWorkerError> {
    std::fs::create_dir_all(root).map_err(failed)?;
    match job.request.protocol {
        DetectedModemProtocol::Xmodem => {
            let (file, file_name) =
                create_download_file(root, "xmodem.bin").map_err(worker_error)?;
            let mut writer = ProgressWriter::new(file, Some(file_name), None, event_tx.clone());
            receive_xmodem(&mut job.transfer, &mut writer, true).map_err(worker_error)?;
        }
        DetectedModemProtocol::Ymodem => {
            receive_ymodem(&mut job.transfer, |header| {
                let total = header.file_size;
                let (file, file_name) = create_download_file(root, &header.file_name)?;
                Ok(ProgressWriter::new(
                    file,
                    Some(file_name),
                    total,
                    event_tx.clone(),
                ))
            })
            .map_err(worker_error)?;
        }
        DetectedModemProtocol::Zmodem => {
            receive_zmodem(&mut job.transfer, |header| {
                let total = header.file_size;
                let (file, file_name) = create_download_file(root, &header.file_name)?;
                Ok(ProgressWriter::new(
                    file,
                    Some(file_name),
                    total,
                    event_tx.clone(),
                ))
            })
            .map_err(worker_error)?;
        }
        DetectedModemProtocol::XymodemNegotiation => {
            return Err(ModemWorkerError::Failed(
                "The remote side is waiting for an X/YMODEM upload, not a download.".to_string(),
            ));
        }
    }
    Ok(())
}

fn run_upload(
    job: &mut ModemWorkerJob,
    paths: &[String],
    event_tx: &std::sync::mpsc::Sender<ModemWorkerEvent>,
) -> Result<(), ModemWorkerError> {
    if paths.is_empty() {
        return Err(ModemWorkerError::Cancelled);
    }

    match job.request.protocol {
        DetectedModemProtocol::Xmodem => {
            let file = File::open(&paths[0]).map_err(failed)?;
            let path = PathBuf::from(&paths[0]);
            let file_size = file.metadata().map_err(failed)?.len();
            let file_name = Some(local_file_name(&path)?);
            let mut file = ProgressReader::new(file, file_name, file_size, event_tx.clone());
            send_xmodem(&mut job.transfer, &mut file, XmodemBlockMode::Bytes1024)
                .map_err(worker_error)?;
        }
        DetectedModemProtocol::XymodemNegotiation if paths.len() == 1 => {
            // Bare C/NAK negotiation does not identify XMODEM vs YMODEM; a
            // single selected file is the least surprising XMODEM fallback.
            let file = File::open(&paths[0]).map_err(failed)?;
            let path = PathBuf::from(&paths[0]);
            let file_size = file.metadata().map_err(failed)?.len();
            let file_name = Some(local_file_name(&path)?);
            let mut file = ProgressReader::new(file, file_name, file_size, event_tx.clone());
            send_xmodem(&mut job.transfer, &mut file, XmodemBlockMode::Bytes1024)
                .map_err(worker_error)?;
        }
        DetectedModemProtocol::XymodemNegotiation | DetectedModemProtocol::Ymodem => {
            let mut entries = open_ymodem_entries(paths, event_tx)?;
            send_ymodem_stream(&mut job.transfer, &mut entries).map_err(worker_error)?;
        }
        DetectedModemProtocol::Zmodem => {
            let mut entries = open_zmodem_entries(paths, event_tx)?;
            send_zmodem_stream(&mut job.transfer, &mut entries).map_err(worker_error)?;
        }
    }
    Ok(())
}

fn open_ymodem_entries(
    paths: &[String],
    event_tx: &std::sync::mpsc::Sender<ModemWorkerEvent>,
) -> Result<Vec<YmodemSendStreamEntry<ProgressReader<File>>>, ModemWorkerError> {
    paths
        .iter()
        .map(|path| {
            let path = PathBuf::from(path);
            let file = File::open(&path).map_err(failed)?;
            let file_size = file.metadata().map_err(failed)?.len();
            let file_name = local_file_name(&path)?;
            let reader =
                ProgressReader::new(file, Some(file_name.clone()), file_size, event_tx.clone());
            Ok(YmodemSendStreamEntry {
                file_name,
                file_size,
                reader,
            })
        })
        .collect()
}

fn open_zmodem_entries(
    paths: &[String],
    event_tx: &std::sync::mpsc::Sender<ModemWorkerEvent>,
) -> Result<Vec<ZmodemSendStreamEntry<ProgressReader<File>>>, ModemWorkerError> {
    paths
        .iter()
        .map(|path| {
            let path = PathBuf::from(path);
            let file = File::open(&path).map_err(failed)?;
            let file_size = file.metadata().map_err(failed)?.len();
            let file_name = local_file_name(&path)?;
            let reader =
                ProgressReader::new(file, Some(file_name.clone()), file_size, event_tx.clone());
            Ok(ZmodemSendStreamEntry {
                file_name,
                file_size,
                reader,
            })
        })
        .collect()
}

fn local_file_name(path: &Path) -> Result<String, ModemWorkerError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| ModemWorkerError::Failed("Invalid local file name".to_string()))
}

fn create_download_file(
    root: &Path,
    remote_name: &str,
) -> Result<(File, String), ModemTransferError> {
    let file_name = safe_download_file_name(remote_name)?;
    for index in 0..10_000 {
        let candidate_name = if index == 0 {
            file_name.clone()
        } else {
            duplicate_download_name(&file_name, index)
        };
        let path = root.join(&candidate_name);
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(file) => return Ok((file, candidate_name)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ModemTransferError::Io(error)),
        }
    }

    Err(ModemTransferError::Io(std::io::Error::new(
        ErrorKind::AlreadyExists,
        "too many duplicate modem download names",
    )))
}

fn safe_download_file_name(remote_name: &str) -> Result<String, ModemTransferError> {
    // Remote file names are untrusted protocol data, so downloads stay under the chosen folder.
    let file_name = Path::new(remote_name)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or(ModemTransferError::Protocol(ModemError::InvalidFileName))?;
    Ok(file_name.to_string())
}

fn duplicate_download_name(file_name: &str, index: usize) -> String {
    let path = Path::new(file_name);
    match (
        path.file_stem().and_then(|stem| stem.to_str()),
        path.extension().and_then(|extension| extension.to_str()),
    ) {
        (Some(stem), Some(extension)) if !stem.is_empty() && !extension.is_empty() => {
            format!("{stem} ({index}).{extension}")
        }
        _ => format!("{file_name} ({index})"),
    }
}

fn failed(error: impl std::fmt::Display) -> ModemWorkerError {
    ModemWorkerError::Failed(error.to_string())
}

fn worker_error(error: ModemTransferError) -> ModemWorkerError {
    match error {
        ModemTransferError::Cancelled => ModemWorkerError::Cancelled,
        error => ModemWorkerError::Failed(error.to_string()),
    }
}

pub(crate) fn format_modem_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= GIB {
        format!("{:.1} GiB", bytes_f / GIB)
    } else if bytes_f >= MIB {
        format!("{:.1} MiB", bytes_f / MIB)
    } else if bytes_f >= KIB {
        format!("{:.1} KiB", bytes_f / KIB)
    } else {
        format!("{bytes} B")
    }
}

struct ProgressReader<R> {
    inner: R,
    file_name: Option<String>,
    transferred_bytes: u64,
    total_bytes: u64,
    event_tx: std::sync::mpsc::Sender<ModemWorkerEvent>,
    last_emit: Instant,
}

impl<R> ProgressReader<R> {
    fn new(
        inner: R,
        file_name: Option<String>,
        total_bytes: u64,
        event_tx: std::sync::mpsc::Sender<ModemWorkerEvent>,
    ) -> Self {
        Self {
            inner,
            file_name,
            transferred_bytes: 0,
            total_bytes,
            event_tx,
            last_emit: Instant::now() - Duration::from_secs(1),
        }
    }

    fn emit_progress(&mut self, force: bool) {
        if !force && self.last_emit.elapsed() < Duration::from_millis(200) {
            return;
        }
        self.last_emit = Instant::now();
        let _ = self
            .event_tx
            .send(ModemWorkerEvent::Progress(ModemWorkerProgress {
                file_name: self.file_name.clone(),
                transferred_bytes: self.transferred_bytes,
                total_bytes: Some(self.total_bytes),
            }));
    }
}

impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.transferred_bytes = self.transferred_bytes.saturating_add(read as u64);
        self.emit_progress(read == 0 || self.transferred_bytes >= self.total_bytes);
        Ok(read)
    }
}

impl<R: Seek> Seek for ProgressReader<R> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let offset = self.inner.seek(position)?;
        self.transferred_bytes = offset;
        self.emit_progress(true);
        Ok(offset)
    }
}

struct ProgressWriter<W> {
    inner: W,
    file_name: Option<String>,
    transferred_bytes: u64,
    total_bytes: Option<u64>,
    event_tx: std::sync::mpsc::Sender<ModemWorkerEvent>,
    last_emit: Instant,
}

impl<W> ProgressWriter<W> {
    fn new(
        inner: W,
        file_name: Option<String>,
        total_bytes: Option<u64>,
        event_tx: std::sync::mpsc::Sender<ModemWorkerEvent>,
    ) -> Self {
        Self {
            inner,
            file_name,
            transferred_bytes: 0,
            total_bytes,
            event_tx,
            last_emit: Instant::now() - Duration::from_secs(1),
        }
    }

    fn emit_progress(&mut self, force: bool) {
        if !force && self.last_emit.elapsed() < Duration::from_millis(200) {
            return;
        }
        self.last_emit = Instant::now();
        let _ = self
            .event_tx
            .send(ModemWorkerEvent::Progress(ModemWorkerProgress {
                file_name: self.file_name.clone(),
                transferred_bytes: self.transferred_bytes,
                total_bytes: self.total_bytes,
            }));
    }
}

impl<W: Write> Write for ProgressWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.transferred_bytes = self.transferred_bytes.saturating_add(written as u64);
        self.emit_progress(
            self.total_bytes
                .is_some_and(|total| self.transferred_bytes >= total),
        );
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
