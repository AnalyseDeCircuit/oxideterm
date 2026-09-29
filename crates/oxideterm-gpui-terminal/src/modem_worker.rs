// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    UploadFiles(Vec<PathBuf>),
    DownloadRoot(PathBuf),
    Cancelled,
}

pub(crate) struct ModemWorkerJob {
    pub transfer: ModemTransfer,
    pub request: TerminalModemTransferRequest,
    pub selection: ModemPromptSelection,
    pub audit: Option<oxideterm_audit::AuditOperation>,
    pub connection_lost: Arc<std::sync::atomic::AtomicBool>,
    pub payload_bytes: Arc<std::sync::atomic::AtomicU64>,
    pub completed_files: Option<u64>,
}

pub(crate) enum ModemWorkerEvent {
    Progress(ModemWorkerProgress),
    Completed,
    Cancelled,
    Failed(ModemFailure),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModemFailure {
    Timeout,
    Protocol,
    FileIo,
    FileTooLarge,
    BufferOverflow,
    WorkerStopped,
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
    if let Some(mut audit) = job.audit.take() {
        let actual_protocol = match (job.request.protocol, &job.selection) {
            (
                DetectedModemProtocol::XymodemNegotiation,
                ModemPromptSelection::UploadFiles(paths),
            ) if paths.len() == 1 => "xmodem",
            (
                DetectedModemProtocol::XymodemNegotiation,
                ModemPromptSelection::UploadFiles(paths),
            ) if !paths.is_empty() => "ymodem",
            (DetectedModemProtocol::XymodemNegotiation, _) => "unknown",
            (DetectedModemProtocol::Xmodem, _) => "xmodem",
            (DetectedModemProtocol::Ymodem, _) => "ymodem",
            (DetectedModemProtocol::Zmodem, _) => "zmodem",
        };
        let selection = match &job.selection {
            ModemPromptSelection::UploadFiles(paths) => format!(
                "local={}; selected={}",
                paths
                    .first()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "unavailable".into()),
                paths.len()
            ),
            ModemPromptSelection::DownloadRoot(root) => format!("local_root={}", root.display()),
            ModemPromptSelection::Cancelled => "selection=cancelled".to_string(),
        };
        audit.summary(&format!("protocol={actual_protocol}; detected={:?}; direction={:?}; {selection}; completed_files={}", job.request.protocol, job.request.direction, job.completed_files.map_or_else(|| "unknown".to_owned(), |count| count.to_string())));
        let bytes = job.payload_bytes.load(std::sync::atomic::Ordering::Relaxed);
        let outcome = if job
            .connection_lost
            .load(std::sync::atomic::Ordering::Acquire)
        {
            oxideterm_audit::AuditOutcome::Interrupted
        } else {
            match &result {
                Ok(()) => oxideterm_audit::AuditOutcome::Succeeded,
                Err(ModemWorkerError::Cancelled) => oxideterm_audit::AuditOutcome::Cancelled,
                Err(_) if bytes > 0 || job.completed_files.is_some_and(|count| count > 0) => {
                    oxideterm_audit::AuditOutcome::Partial
                }
                Err(_) => oxideterm_audit::AuditOutcome::Failed,
            }
        };
        audit.finish(
            outcome,
            oxideterm_audit::AuditEvidence::Protocol,
            None,
            Some(bytes),
        );
    }
    if result.is_err() {
        // Protocol failures and user cancellation both need an on-wire abort so
        // the peer exits instead of retrying after the UI releases the transfer.
        job.transfer.stop();
    }
    let event = match result {
        Ok(()) => ModemWorkerEvent::Completed,
        Err(ModemWorkerError::Cancelled) => ModemWorkerEvent::Cancelled,
        Err(ModemWorkerError::Failed(message)) => ModemWorkerEvent::Failed(message),
    };
    let _ = event_tx.send(event);
}

enum ModemWorkerError {
    Cancelled,
    Failed(ModemFailure),
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
            run_download(job, &root, event_tx)
        }
        (ModemTransferDirection::Upload, ModemPromptSelection::UploadFiles(paths)) => {
            run_upload(job, &paths, event_tx)
        }
        _ => Err(ModemWorkerError::Failed(ModemFailure::FileIo)),
    }
}

fn run_download(
    job: &mut ModemWorkerJob,
    root: &Path,
    event_tx: &std::sync::mpsc::Sender<ModemWorkerEvent>,
) -> Result<(), ModemWorkerError> {
    std::fs::create_dir_all(root).map_err(failed)?;
    let downloads = DownloadBatch::default();
    match job.request.protocol {
        DetectedModemProtocol::Xmodem => {
            let (file, file_name) = downloads
                .create_writer(root, "xmodem.bin")
                .map_err(worker_error)?;
            let mut writer = ProgressWriter::new(
                file,
                Some(file_name),
                None,
                event_tx.clone(),
                job.payload_bytes.clone(),
            );
            receive_xmodem(&mut job.transfer, &mut writer, true).map_err(worker_error)?;
            drop(writer);
        }
        DetectedModemProtocol::Ymodem => {
            receive_ymodem(&mut job.transfer, |header| {
                let total = header.file_size;
                let (file, file_name) = downloads.create_writer(root, &header.file_name)?;
                Ok(ProgressWriter::new(
                    file,
                    Some(file_name),
                    total,
                    event_tx.clone(),
                    job.payload_bytes.clone(),
                ))
            })
            .map_err(worker_error)?;
        }
        DetectedModemProtocol::Zmodem => {
            receive_zmodem(&mut job.transfer, |header| {
                let total = header.file_size;
                let (file, file_name) = downloads.create_writer(root, &header.file_name)?;
                Ok(ProgressWriter::new(
                    file,
                    Some(file_name),
                    total,
                    event_tx.clone(),
                    job.payload_bytes.clone(),
                ))
            })
            .map_err(worker_error)?;
        }
        DetectedModemProtocol::XymodemNegotiation => {
            return Err(ModemWorkerError::Failed(ModemFailure::Protocol));
        }
    }
    downloads
        .commit(root, &mut job.completed_files)
        .map_err(worker_error)?;
    Ok(())
}

fn run_upload(
    job: &mut ModemWorkerJob,
    paths: &[PathBuf],
    event_tx: &std::sync::mpsc::Sender<ModemWorkerEvent>,
) -> Result<(), ModemWorkerError> {
    if paths.is_empty() {
        return Err(ModemWorkerError::Cancelled);
    }

    match job.request.protocol {
        DetectedModemProtocol::Xmodem => {
            let path = &paths[0];
            let file = File::open(path).map_err(failed)?;
            let file_size = file.metadata().map_err(failed)?.len();
            let file_name = Some(local_file_name(path)?);
            let mut file = ProgressReader::new(
                file,
                file_name,
                file_size,
                event_tx.clone(),
                job.payload_bytes.clone(),
            );
            send_xmodem(&mut job.transfer, &mut file, XmodemBlockMode::Bytes1024)
                .map_err(worker_error)?;
            job.completed_files = Some(1);
        }
        DetectedModemProtocol::XymodemNegotiation if paths.len() == 1 => {
            // Bare C/NAK negotiation does not identify XMODEM vs YMODEM; a
            // single selected file is the least surprising XMODEM fallback.
            let path = &paths[0];
            let file = File::open(path).map_err(failed)?;
            let file_size = file.metadata().map_err(failed)?.len();
            let file_name = Some(local_file_name(path)?);
            let mut file = ProgressReader::new(
                file,
                file_name,
                file_size,
                event_tx.clone(),
                job.payload_bytes.clone(),
            );
            send_xmodem(&mut job.transfer, &mut file, XmodemBlockMode::Bytes1024)
                .map_err(worker_error)?;
            job.completed_files = Some(1);
        }
        DetectedModemProtocol::XymodemNegotiation | DetectedModemProtocol::Ymodem => {
            let mut entries = open_ymodem_entries(paths, event_tx, job.payload_bytes.clone())?;
            send_ymodem_stream(&mut job.transfer, &mut entries).map_err(worker_error)?;
            job.completed_files = Some(paths.len() as u64);
        }
        DetectedModemProtocol::Zmodem => {
            let mut entries = open_zmodem_entries(paths, event_tx, job.payload_bytes.clone())?;
            send_zmodem_stream(&mut job.transfer, &mut entries).map_err(worker_error)?;
            job.completed_files = Some(paths.len() as u64);
        }
    }
    Ok(())
}

fn open_ymodem_entries(
    paths: &[PathBuf],
    event_tx: &std::sync::mpsc::Sender<ModemWorkerEvent>,
    payload_bytes: Arc<std::sync::atomic::AtomicU64>,
) -> Result<Vec<YmodemSendStreamEntry<ProgressReader<File>>>, ModemWorkerError> {
    paths
        .iter()
        .map(|path| {
            let file = File::open(path).map_err(failed)?;
            let file_size = file.metadata().map_err(failed)?.len();
            let file_name = local_file_name(path)?;
            let reader = ProgressReader::new(
                file,
                Some(file_name.clone()),
                file_size,
                event_tx.clone(),
                payload_bytes.clone(),
            );
            Ok(YmodemSendStreamEntry {
                file_name,
                file_size,
                reader,
            })
        })
        .collect()
}

fn open_zmodem_entries(
    paths: &[PathBuf],
    event_tx: &std::sync::mpsc::Sender<ModemWorkerEvent>,
    payload_bytes: Arc<std::sync::atomic::AtomicU64>,
) -> Result<Vec<ZmodemSendStreamEntry<ProgressReader<File>>>, ModemWorkerError> {
    paths
        .iter()
        .map(|path| {
            let file = File::open(path).map_err(failed)?;
            let file_size = file.metadata().map_err(failed)?.len();
            let file_name = local_file_name(path)?;
            let reader = ProgressReader::new(
                file,
                Some(file_name.clone()),
                file_size,
                event_tx.clone(),
                payload_bytes.clone(),
            );
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
        .filter(|name| !name.is_empty())
        // The filesystem path remains lossless; only the protocol metadata is
        // converted because classic modem filename fields are byte strings.
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or(ModemWorkerError::Failed(ModemFailure::FileIo))
}

#[derive(Clone, Default)]
struct DownloadBatch {
    pending: Arc<parking_lot::Mutex<Vec<PendingDownload>>>,
}

struct PendingDownload {
    temporary_file: tempfile::NamedTempFile,
    requested_name: String,
}

struct TransactionalDownloadWriter {
    temporary_file: Option<tempfile::NamedTempFile>,
    requested_name: String,
    pending: Arc<parking_lot::Mutex<Vec<PendingDownload>>>,
}

impl DownloadBatch {
    fn create_writer(
        &self,
        root: &Path,
        remote_name: &str,
    ) -> Result<(TransactionalDownloadWriter, String), ModemTransferError> {
        let requested_name = safe_download_file_name(remote_name)?;
        let temporary_file = tempfile::Builder::new()
            .prefix(".oxideterm-modem-")
            .suffix(".part")
            .tempfile_in(root)?;
        Ok((
            TransactionalDownloadWriter {
                temporary_file: Some(temporary_file),
                requested_name: requested_name.clone(),
                pending: self.pending.clone(),
            },
            requested_name,
        ))
    }

    fn commit(&self, root: &Path, completed: &mut Option<u64>) -> Result<(), ModemTransferError> {
        let pending = std::mem::take(&mut *self.pending.lock());
        for pending_download in pending {
            persist_download_without_overwrite(root, pending_download)?;
            *completed = Some(completed.unwrap_or(0).saturating_add(1));
        }
        Ok(())
    }
}

impl Write for TransactionalDownloadWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.temporary_file
            .as_mut()
            .expect("transactional download file")
            .write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.temporary_file
            .as_mut()
            .expect("transactional download file")
            .flush()
    }
}

impl Drop for TransactionalDownloadWriter {
    fn drop(&mut self) {
        let Some(temporary_file) = self.temporary_file.take() else {
            return;
        };
        self.pending.lock().push(PendingDownload {
            temporary_file,
            requested_name: self.requested_name.clone(),
        });
    }
}

fn persist_download_without_overwrite(
    root: &Path,
    mut pending: PendingDownload,
) -> Result<(), ModemTransferError> {
    for index in 0..10_000 {
        let candidate_name = if index == 0 {
            pending.requested_name.clone()
        } else {
            duplicate_download_name(&pending.requested_name, index)
        };
        match pending
            .temporary_file
            .persist_noclobber(root.join(candidate_name))
        {
            Ok(file) => {
                drop(file);
                return Ok(());
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                pending.temporary_file = error.file;
            }
            Err(error) => return Err(ModemTransferError::Io(error.error)),
        }
    }

    Err(ModemTransferError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
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

fn failed(_error: std::io::Error) -> ModemWorkerError {
    // File errors may contain paths; only a safe category crosses into UI notices.
    ModemWorkerError::Failed(ModemFailure::FileIo)
}

fn worker_error(error: ModemTransferError) -> ModemWorkerError {
    match error {
        ModemTransferError::Cancelled => ModemWorkerError::Cancelled,
        ModemTransferError::Timeout => ModemWorkerError::Failed(ModemFailure::Timeout),
        ModemTransferError::Io(_) => ModemWorkerError::Failed(ModemFailure::FileIo),
        ModemTransferError::UnsupportedFileSize(_) => {
            ModemWorkerError::Failed(ModemFailure::FileTooLarge)
        }
        ModemTransferError::InputBufferOverflow(_) => {
            ModemWorkerError::Failed(ModemFailure::BufferOverflow)
        }
        ModemTransferError::Protocol(_)
        | ModemTransferError::UnexpectedByte(_)
        | ModemTransferError::UnexpectedFrame => ModemWorkerError::Failed(ModemFailure::Protocol),
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
    payload_bytes: Arc<std::sync::atomic::AtomicU64>,
}

impl<R> ProgressReader<R> {
    fn new(
        inner: R,
        file_name: Option<String>,
        total_bytes: u64,
        event_tx: std::sync::mpsc::Sender<ModemWorkerEvent>,
        payload_bytes: Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        Self {
            inner,
            file_name,
            transferred_bytes: 0,
            total_bytes,
            event_tx,
            last_emit: Instant::now() - Duration::from_secs(1),
            payload_bytes,
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
        self.payload_bytes
            .fetch_add(read as u64, std::sync::atomic::Ordering::Relaxed);
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
    payload_bytes: Arc<std::sync::atomic::AtomicU64>,
}

impl<W> ProgressWriter<W> {
    fn new(
        inner: W,
        file_name: Option<String>,
        total_bytes: Option<u64>,
        event_tx: std::sync::mpsc::Sender<ModemWorkerEvent>,
        payload_bytes: Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        Self {
            inner,
            file_name,
            transferred_bytes: 0,
            total_bytes,
            event_tx,
            last_emit: Instant::now() - Duration::from_secs(1),
            payload_bytes,
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
        self.payload_bytes
            .fetch_add(written as u64, std::sync::atomic::Ordering::Relaxed);
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

#[cfg(test)]
mod tests {
    use super::*;

    struct TestKeys;

    impl oxideterm_audit::AuditKeyProvider for TestKeys {
        fn load(
            &self,
            _: &str,
        ) -> Result<zeroize::Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
            Ok(zeroize::Zeroizing::new(vec![11; 32]))
        }

        fn create(
            &self,
            id: &str,
        ) -> Result<zeroize::Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
            self.load(id)
        }
    }

    #[test]
    fn modem_worker_distinguishes_cancel_from_connection_loss() {
        use oxideterm_audit::{
            AuditCategory, AuditContext, AuditOutcome, AuditQuery, AuditService, AuditSource,
            AuditStore,
        };

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("audit.db");
        oxideterm_audit::AuditStore::open(&path, &TestKeys)
            .unwrap()
            .set_policy(oxideterm_audit::AuditPolicy {
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let service = AuditService::with_key_provider(path.clone(), TestKeys).unwrap();
        for (source, connection_lost) in [(AuditSource::User, false), (AuditSource::System, true)] {
            let context = AuditContext::new(service.client(), source);
            let audit = context.operation(
                AuditCategory::File,
                "file_transfer",
                Some("protocol=xmodem"),
            );
            let (sender, receiver) = std::sync::mpsc::channel();
            run_modem_worker_job(
                ModemWorkerJob {
                    transfer: ModemTransfer::new(&[]),
                    request: TerminalModemTransferRequest {
                        protocol: DetectedModemProtocol::Xmodem,
                        direction: ModemTransferDirection::Upload,
                    },
                    selection: ModemPromptSelection::Cancelled,
                    audit: Some(audit),
                    connection_lost: Arc::new(std::sync::atomic::AtomicBool::new(connection_lost)),
                    payload_bytes: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                    completed_files: None,
                },
                sender,
            );
            assert!(matches!(
                receiver.recv().unwrap(),
                ModemWorkerEvent::Cancelled
            ));
        }
        drop(service);
        let records = AuditStore::open(&path, &TestKeys)
            .unwrap()
            .query(&AuditQuery {
                category: Some(AuditCategory::File),
                limit: 16,
                ..Default::default()
            })
            .unwrap()
            .records;
        let outcomes = records
            .iter()
            .filter_map(|record| record.details.operation.as_ref())
            .filter(|operation| operation.phase == Some(oxideterm_audit::AuditPhase::Result))
            .map(|operation| (operation.source, operation.outcome, operation.bytes))
            .collect::<Vec<_>>();
        assert!(outcomes.contains(&(AuditSource::User, AuditOutcome::Cancelled, Some(0))));
        assert!(outcomes.contains(&(AuditSource::System, AuditOutcome::Interrupted, Some(0))));
    }

    #[test]
    fn modem_payload_counter_tracks_bytes_processed_by_file_io() {
        let (sender, _receiver) = std::sync::mpsc::channel();
        let bytes = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut reader = ProgressReader::new(
            std::io::Cursor::new(b"hello".to_vec()),
            Some("hello.txt".into()),
            5,
            sender.clone(),
            bytes.clone(),
        );
        let mut buffer = [0; 3];
        assert_eq!(reader.read(&mut buffer).unwrap(), 3);
        assert_eq!(reader.read(&mut buffer).unwrap(), 2);
        let mut writer = ProgressWriter::new(
            Vec::new(),
            Some("reply.txt".into()),
            Some(4),
            sender,
            bytes.clone(),
        );
        writer.write_all(b"data").unwrap();
        assert_eq!(bytes.load(std::sync::atomic::Ordering::Relaxed), 9);
    }

    #[test]
    fn abandoned_download_batch_removes_partial_files() {
        let root = tempfile::tempdir().unwrap();
        let downloads = DownloadBatch::default();
        let (mut writer, _) = downloads.create_writer(root.path(), "partial.bin").unwrap();
        writer.write_all(b"incomplete").unwrap();
        drop(writer);
        drop(downloads);

        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn committed_download_does_not_overwrite_an_existing_file() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("report.txt"), b"existing").unwrap();
        let downloads = DownloadBatch::default();
        let (mut writer, _) = downloads.create_writer(root.path(), "report.txt").unwrap();
        writer.write_all(b"downloaded").unwrap();
        drop(writer);

        let mut completed = None;
        downloads.commit(root.path(), &mut completed).unwrap();
        assert_eq!(completed, Some(1));

        assert_eq!(
            std::fs::read(root.path().join("report.txt")).unwrap(),
            b"existing"
        );
        assert_eq!(
            std::fs::read(root.path().join("report (1).txt")).unwrap(),
            b"downloaded"
        );
    }

    #[test]
    fn remote_paths_are_reduced_to_safe_file_names() {
        assert_eq!(
            safe_download_file_name("../../report.txt").unwrap(),
            "report.txt"
        );
        assert!(safe_download_file_name("..").is_err());
        assert!(safe_download_file_name("/").is_err());
    }
}
