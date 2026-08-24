// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use flate2::read::ZlibDecoder;
use oxideterm_atomic_file::durable_replace;
use tempfile::NamedTempFile;

use super::*;

const TIGHT_VENDOR: [u8; 4] = *b"TGHT";
const MAX_TIGHT_CAPABILITIES: usize = 256;
const MAX_VNC_FILE_COUNT: usize = 128;
const MAX_VNC_FILE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_VNC_FILE_NAME_BYTES: usize = 255;
const MAX_VNC_REMOTE_PATH_BYTES: usize = 4095;
const VNC_FILE_CHUNK_BYTES: usize = 60 * 1024;
const VNC_DOWNLOAD_PROGRESS_INTERVAL_BYTES: u64 = 1024 * 1024;
const VNC_DIRECTORY_SIZE_MARKER: u32 = u32::MAX;

const FILE_LIST_DATA: TightCapability = TightCapability::new(130, *b"TGHT", *b"FTS_LSDT");
const FILE_DOWNLOAD_DATA: TightCapability = TightCapability::new(131, *b"TGHT", *b"FTS_DNDT");
pub(super) const FILE_UPLOAD_CANCEL: TightCapability =
    TightCapability::new(132, *b"TGHT", *b"FTS_UPCN");
const FILE_DOWNLOAD_FAILED: TightCapability = TightCapability::new(133, *b"TGHT", *b"FTS_DNFL");
const FILE_LIST_REQUEST: TightCapability = TightCapability::new(130, *b"TGHT", *b"FTC_LSRQ");
const FILE_DOWNLOAD_REQUEST: TightCapability = TightCapability::new(131, *b"TGHT", *b"FTC_DNRQ");
const FILE_UPLOAD_REQUEST: TightCapability = TightCapability::new(132, *b"TGHT", *b"FTC_UPRQ");
const FILE_UPLOAD_DATA: TightCapability = TightCapability::new(133, *b"TGHT", *b"FTC_UPDT");
const FILE_DOWNLOAD_CANCEL: TightCapability = TightCapability::new(134, *b"TGHT", *b"FTC_DNCN");
const FILE_UPLOAD_FAILED: TightCapability = TightCapability::new(135, *b"TGHT", *b"FTC_UPFL");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TightCapability {
    pub(super) code: i32,
    pub(super) vendor: [u8; 4],
    pub(super) signature: [u8; 8],
}

impl TightCapability {
    pub(super) const fn new(code: i32, vendor: [u8; 4], signature: [u8; 8]) -> Self {
        Self {
            code,
            vendor,
            signature,
        }
    }

    pub(super) fn is_exact(self, code: i32, vendor: [u8; 4], signature: [u8; 8]) -> bool {
        self.code == code && self.vendor == vendor && self.signature == signature
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct TightInteractionCapabilities {
    pub(super) server_messages: Vec<TightCapability>,
    pub(super) client_messages: Vec<TightCapability>,
    pub(super) encodings: Vec<TightCapability>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct TightFileCapabilities {
    pub(super) list: bool,
    pub(super) download: bool,
    pub(super) upload: bool,
}

impl TightFileCapabilities {
    pub(super) fn from_interaction(capabilities: &TightInteractionCapabilities) -> Self {
        let has_server =
            |expected: TightCapability| capabilities.server_messages.contains(&expected);
        let has_client =
            |expected: TightCapability| capabilities.client_messages.contains(&expected);
        Self {
            list: has_server(FILE_LIST_DATA) && has_client(FILE_LIST_REQUEST),
            download: has_server(FILE_DOWNLOAD_DATA)
                && has_server(FILE_DOWNLOAD_FAILED)
                && has_client(FILE_DOWNLOAD_REQUEST)
                && has_client(FILE_DOWNLOAD_CANCEL),
            upload: has_server(FILE_UPLOAD_CANCEL)
                && has_client(FILE_UPLOAD_REQUEST)
                && has_client(FILE_UPLOAD_DATA)
                && has_client(FILE_UPLOAD_FAILED),
        }
    }
}

pub(super) fn read_tight_capability(reader: &mut impl Read) -> Result<TightCapability, String> {
    let bytes = read_exact_array::<16, _>(reader)
        .map_err(|error| format!("VNC Tight capability read failed: {error}"))?;
    let mut vendor = [0; 4];
    vendor.copy_from_slice(&bytes[4..8]);
    let mut signature = [0; 8];
    signature.copy_from_slice(&bytes[8..16]);
    Ok(TightCapability::new(be_i32(&bytes[..4]), vendor, signature))
}

pub(super) fn read_tight_capability_list(
    reader: &mut impl Read,
    count: usize,
) -> Result<Vec<TightCapability>, String> {
    if count > MAX_TIGHT_CAPABILITIES {
        return Err("VNC Tight capability count exceeds the helper limit.".to_string());
    }
    (0..count).map(|_| read_tight_capability(reader)).collect()
}

pub(super) fn read_tight_interaction_capabilities(
    reader: &mut impl Read,
) -> Result<TightInteractionCapabilities, String> {
    let header = read_exact_array::<8, _>(reader)
        .map_err(|error| format!("VNC Tight interaction header read failed: {error}"))?;
    let server_count = usize::from(be_u16(&header[..2]));
    let client_count = usize::from(be_u16(&header[2..4]));
    let encoding_count = usize::from(be_u16(&header[4..6]));
    if header[6..8] != [0, 0] {
        return Err("VNC Tight interaction header padding is invalid.".to_string());
    }
    let total = server_count
        .checked_add(client_count)
        .and_then(|count| count.checked_add(encoding_count))
        .ok_or_else(|| "VNC Tight capability count overflowed.".to_string())?;
    if total > MAX_TIGHT_CAPABILITIES {
        return Err("VNC Tight capability count exceeds the helper limit.".to_string());
    }
    Ok(TightInteractionCapabilities {
        server_messages: read_tight_capability_list(reader, server_count)?,
        client_messages: read_tight_capability_list(reader, client_count)?,
        encodings: read_tight_capability_list(reader, encoding_count)?,
    })
}

#[derive(Default)]
pub(super) struct VncVendorFileActions {
    pub(super) messages: Vec<Vec<u8>>,
    pub(super) events: Vec<RemoteDesktopHelperEvent>,
}

struct PendingFileList {
    request_id: String,
    path: String,
}

struct DownloadPlan {
    entry: RemoteDesktopRemoteFileEntry,
    target_path: PathBuf,
}

struct ActiveDownloadFile {
    plan: DownloadPlan,
    temporary_file: NamedTempFile,
    received_bytes: u64,
}

struct DownloadBatch {
    transfer_id: String,
    pending: VecDeque<DownloadPlan>,
    current: Option<ActiveDownloadFile>,
    conflict_policy: RemoteDesktopFileConflictPolicy,
    total_bytes: u64,
    transferred_bytes: u64,
    total_files: u32,
    completed_files: u32,
    skipped_files: u32,
    completed_paths: Vec<PathBuf>,
    last_reported_bytes: u64,
}

impl DownloadBatch {
    fn start_next(&mut self) -> Result<Option<Vec<u8>>, String> {
        let Some(plan) = self.pending.pop_front() else {
            return Ok(None);
        };
        let parent = plan
            .target_path
            .parent()
            .ok_or_else(|| "VNC download target has no parent directory.".to_string())?;
        let temporary_file = tempfile::Builder::new()
            .prefix(".oxideterm-vnc-")
            .suffix(".part")
            .tempfile_in(parent)
            .map_err(|error| format!("VNC download temporary file creation failed: {error}"))?;
        let request = encode_download_request(&plan.entry.path)?;
        self.current = Some(ActiveDownloadFile {
            plan,
            temporary_file,
            received_bytes: 0,
        });
        Ok(Some(request))
    }
}

pub(super) struct VncVendorFileSession {
    capabilities: TightFileCapabilities,
    canceled_transfers: HashSet<String>,
    active_upload: Option<String>,
    pending_list: Option<PendingFileList>,
    listings: HashMap<String, Vec<RemoteDesktopRemoteFileEntry>>,
    active_download: Option<DownloadBatch>,
    canceling_download: Option<String>,
}

impl Default for VncVendorFileSession {
    fn default() -> Self {
        Self::new(TightFileCapabilities::default())
    }
}

impl VncVendorFileSession {
    pub(super) fn new(capabilities: TightFileCapabilities) -> Self {
        Self {
            capabilities,
            canceled_transfers: HashSet::new(),
            active_upload: None,
            pending_list: None,
            listings: HashMap::new(),
            active_download: None,
            canceling_download: None,
        }
    }

    pub(super) fn cancel_upload(&mut self, transfer_id: String) -> Option<Vec<u8>> {
        const REASON: &[u8] = b"Canceled.";
        if self.active_upload.as_deref() == Some(transfer_id.as_str()) {
            self.active_upload = None;
            return Some(file_failure_message(FILE_UPLOAD_FAILED.code as u8, REASON));
        }
        self.canceled_transfers.insert(transfer_id);
        None
    }

    pub(super) fn request_list(
        &mut self,
        request_id: String,
        path: String,
    ) -> Result<Vec<u8>, String> {
        if !self.capabilities.list {
            return Err("VNC server did not negotiate Tight file listing.".to_string());
        }
        if self.pending_list.is_some() {
            return Err("VNC file listing request is already active.".to_string());
        }
        if !path.is_empty() {
            validate_remote_path(&path)?;
        }
        let message = encode_file_list_request(&path)?;
        self.pending_list = Some(PendingFileList { request_id, path });
        Ok(message)
    }

    pub(super) fn start_download(
        &mut self,
        transfer_id: String,
        remote_paths: Vec<String>,
        destination: PathBuf,
        conflict_policy: RemoteDesktopFileConflictPolicy,
    ) -> Result<VncVendorFileActions, String> {
        if !self.capabilities.download {
            return Err("VNC server did not negotiate Tight file download.".to_string());
        }
        if self.active_download.is_some() || self.canceling_download.is_some() {
            return Err("VNC file download is already active.".to_string());
        }
        if remote_paths.is_empty() || remote_paths.len() > MAX_VNC_FILE_COUNT {
            return Err("VNC file download count is outside the helper limit.".to_string());
        }
        let destination = fs::canonicalize(&destination)
            .map_err(|error| format!("VNC download destination resolution failed: {error}"))?;
        if !destination.is_dir() {
            return Err("VNC download destination is not a directory.".to_string());
        }

        let mut requested_paths = HashSet::new();
        let mut reserved_targets = HashSet::new();
        let mut pending = VecDeque::new();
        let mut skipped_files = 0u32;
        let mut total_bytes = 0u64;
        for remote_path in remote_paths {
            if !requested_paths.insert(remote_path.clone()) {
                return Err("VNC file download request contains a duplicate path.".to_string());
            }
            let entry = self
                .listings
                .values()
                .flatten()
                .find(|entry| entry.path == remote_path)
                .filter(|entry| entry.kind == RemoteDesktopRemoteFileKind::File)
                .cloned()
                .ok_or_else(|| {
                    "VNC download path was not present in a negotiated directory listing."
                        .to_string()
                })?;
            validate_remote_file_name(&entry.name)?;
            let requested_target = destination.join(&entry.name);
            let target_path = match conflict_policy {
                RemoteDesktopFileConflictPolicy::Overwrite => requested_target,
                RemoteDesktopFileConflictPolicy::Skip if requested_target.exists() => {
                    skipped_files = skipped_files.saturating_add(1);
                    continue;
                }
                RemoteDesktopFileConflictPolicy::Skip => requested_target,
                RemoteDesktopFileConflictPolicy::Rename => {
                    unique_download_target(&requested_target, &reserved_targets)?
                }
            };
            if !reserved_targets.insert(target_path.clone()) {
                return Err("VNC file download resolves to a duplicate local target.".to_string());
            }
            total_bytes = total_bytes
                .checked_add(entry.size.unwrap_or(0))
                .ok_or_else(|| "VNC file download size overflowed.".to_string())?;
            pending.push_back(DownloadPlan { entry, target_path });
        }

        let total_files = u32::try_from(pending.len())
            .unwrap_or(u32::MAX)
            .saturating_add(skipped_files);
        let mut batch = DownloadBatch {
            transfer_id: transfer_id.clone(),
            pending,
            current: None,
            conflict_policy,
            total_bytes,
            transferred_bytes: 0,
            total_files,
            completed_files: 0,
            skipped_files,
            completed_paths: Vec::new(),
            last_reported_bytes: 0,
        };
        let mut actions = VncVendorFileActions::default();
        if let Some(message) = batch.start_next()? {
            actions.messages.push(message);
            self.active_download = Some(batch);
        } else {
            actions
                .events
                .push(RemoteDesktopHelperEvent::VncFileTransferCompleted {
                    transfer_id,
                    paths: Vec::new(),
                    skipped_files,
                });
        }
        Ok(actions)
    }

    pub(super) fn cancel_download(&mut self, transfer_id: String) -> VncVendorFileActions {
        let mut actions = VncVendorFileActions::default();
        if self
            .active_download
            .as_ref()
            .is_some_and(|download| download.transfer_id == transfer_id)
        {
            self.active_download = None;
            self.canceling_download = Some(transfer_id.clone());
            actions.messages.push(file_failure_message(
                FILE_DOWNLOAD_CANCEL.code as u8,
                b"Canceled.",
            ));
            actions
                .events
                .push(RemoteDesktopHelperEvent::VncFileTransferFailed {
                    transfer_id,
                    kind: RemoteDesktopFileTransferFailureKind::Canceled,
                });
        }
        actions
    }

    pub(super) fn upload_payload(
        &mut self,
        transfer_id: &str,
        paths: &[PathBuf],
    ) -> Result<Vec<u8>, String> {
        if !self.capabilities.upload {
            return Err("VNC server did not negotiate Tight file upload.".to_string());
        }
        if paths.is_empty() || paths.len() > MAX_VNC_FILE_COUNT {
            return Err("VNC file upload count is outside the helper limit.".to_string());
        }

        self.active_upload = Some(transfer_id.to_string());
        // One owned payload keeps a multi-chunk upload atomic at the bounded
        // writer queue. RFB still sees the individual concatenated messages.
        let mut payload = Vec::new();
        let mut total_size = 0u64;
        for path in paths {
            if self.canceled_transfers.remove(transfer_id) {
                return Err("VNC file upload was canceled.".to_string());
            }
            let source = validate_local_upload_file(path)?;
            total_size = total_size
                .checked_add(source.size)
                .ok_or_else(|| "VNC file upload size overflowed.".to_string())?;
            if total_size > MAX_VNC_FILE_BYTES {
                return Err("VNC file upload exceeds the 20 MiB total limit.".to_string());
            }
            for message in encode_tight_upload_file(&source)? {
                payload.extend_from_slice(&message);
            }
        }
        Ok(payload)
    }

    pub(super) fn observe_server_message(
        &mut self,
        message_type: u8,
        reader: &mut impl Read,
    ) -> Result<VncVendorFileActions, String> {
        match message_type {
            130 if self.capabilities.list => self.read_file_list(reader),
            131 if self.capabilities.download => self.read_download_data(reader),
            132 if self.capabilities.upload => self.read_upload_cancel(reader),
            133 if self.capabilities.download => self.read_download_failed(reader),
            _ => Err("VNC server sent an unnegotiated Tight file-transfer message.".to_string()),
        }
    }

    pub(super) fn accepts_server_message(&self, message_type: u8) -> bool {
        match message_type {
            130 => self.capabilities.list,
            131 | 133 => self.capabilities.download,
            132 => self.capabilities.upload,
            _ => false,
        }
    }

    fn read_file_list(&mut self, reader: &mut impl Read) -> Result<VncVendorFileActions, String> {
        let header = read_exact_array::<7, _>(reader)
            .map_err(|error| format!("VNC file list header read failed: {error}"))?;
        let flags = header[0];
        let entry_count = usize::from(be_u16(&header[1..3]));
        let names_size = usize::from(be_u16(&header[3..5]));
        let wire_names_size = usize::from(be_u16(&header[5..7]));
        if entry_count > MAX_VNC_FILE_COUNT {
            return Err("VNC file list exceeds the helper entry limit.".to_string());
        }
        let metadata_size = entry_count
            .checked_mul(8)
            .ok_or_else(|| "VNC file list metadata size overflowed.".to_string())?;
        let metadata = read_exact_vec(reader, metadata_size)
            .map_err(|error| format!("VNC file list metadata read failed: {error}"))?;
        let wire_names = read_exact_vec(reader, wire_names_size)
            .map_err(|error| format!("VNC file list names read failed: {error}"))?;
        let Some(pending) = self.pending_list.take() else {
            return Err("VNC server sent a file list without an active request.".to_string());
        };
        if flags & 0x80 != 0 {
            return Ok(VncVendorFileActions {
                messages: Vec::new(),
                events: vec![RemoteDesktopHelperEvent::VncRemoteFileListFailed {
                    request_id: pending.request_id,
                }],
            });
        }

        let names = decode_file_list_names(&wire_names, names_size)?;
        let mut name_offset = 0usize;
        let mut entries = Vec::with_capacity(entry_count);
        for index in 0..entry_count {
            let remaining = &names[name_offset..];
            let name_size = remaining
                .iter()
                .position(|byte| *byte == 0)
                .ok_or_else(|| "VNC file list entry name is not terminated.".to_string())?;
            let name = std::str::from_utf8(&remaining[..name_size])
                .map_err(|_| "VNC file list entry name is not valid UTF-8.".to_string())?;
            validate_remote_file_name(name)?;
            name_offset = name_offset
                .checked_add(name_size + 1)
                .ok_or_else(|| "VNC file list name offset overflowed.".to_string())?;

            let metadata_offset = index * 8;
            let size = be_u32(&metadata[metadata_offset..metadata_offset + 4]);
            let modified_seconds = be_u32(&metadata[metadata_offset + 4..metadata_offset + 8]);
            let kind = if size == VNC_DIRECTORY_SIZE_MARKER {
                RemoteDesktopRemoteFileKind::Directory
            } else {
                RemoteDesktopRemoteFileKind::File
            };
            entries.push(RemoteDesktopRemoteFileEntry {
                name: name.to_string(),
                path: remote_child_path(&pending.path, name),
                kind,
                size: (kind == RemoteDesktopRemoteFileKind::File).then_some(u64::from(size)),
                modified_seconds: (kind == RemoteDesktopRemoteFileKind::File)
                    .then_some(u64::from(modified_seconds)),
            });
        }
        if name_offset != names.len() {
            return Err("VNC file list contains trailing name data.".to_string());
        }
        self.listings.insert(pending.path.clone(), entries.clone());
        Ok(VncVendorFileActions {
            messages: Vec::new(),
            events: vec![RemoteDesktopHelperEvent::VncRemoteFilesListed {
                request_id: pending.request_id,
                path: pending.path,
                entries,
            }],
        })
    }

    fn read_download_data(
        &mut self,
        reader: &mut impl Read,
    ) -> Result<VncVendorFileActions, String> {
        let header = read_exact_array::<5, _>(reader)
            .map_err(|error| format!("VNC file download header read failed: {error}"))?;
        let compression_level = header[0];
        let real_size = usize::from(be_u16(&header[1..3]));
        let wire_size = usize::from(be_u16(&header[3..5]));
        if self.canceling_download.is_some() {
            if real_size == 0 && wire_size == 0 {
                read_exact_array::<4, _>(reader).map_err(|error| {
                    format!("VNC canceled download timestamp read failed: {error}")
                })?;
                self.canceling_download = None;
            } else if real_size == 0 || wire_size == 0 {
                return Err("VNC canceled download block has inconsistent sizes.".to_string());
            } else {
                read_exact_vec(reader, wire_size)
                    .map_err(|error| format!("VNC canceled download block read failed: {error}"))?;
            }
            return Ok(VncVendorFileActions::default());
        }
        if real_size == 0 && wire_size == 0 {
            let modified_bytes = read_exact_array::<4, _>(reader)
                .map_err(|error| format!("VNC file download timestamp read failed: {error}"))?;
            // TightVNC and LibVNCServer historically send this lone field in
            // host order even though the surrounding header uses network order.
            let modified_seconds = u32::from_le_bytes(modified_bytes);
            return self.finish_download_file(modified_seconds);
        }
        if real_size == 0 || wire_size == 0 {
            return Err("VNC file download block has inconsistent sizes.".to_string());
        }
        let wire = read_exact_vec(reader, wire_size)
            .map_err(|error| format!("VNC file download block read failed: {error}"))?;
        let bytes = if compression_level == 0 {
            if real_size != wire_size {
                return Err("VNC uncompressed download block has mismatched sizes.".to_string());
            }
            wire
        } else {
            decode_zlib_exact(&wire, real_size, "VNC file download block")?
        };

        let Some(batch) = self.active_download.as_mut() else {
            return Err("VNC server sent download data without an active transfer.".to_string());
        };
        let Some(current) = batch.current.as_mut() else {
            return Err("VNC download transfer has no active file.".to_string());
        };
        let next_file_bytes = current
            .received_bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| "VNC file download size overflowed.".to_string())?;
        let expected_size = current.plan.entry.size.unwrap_or(u64::from(u32::MAX));
        if next_file_bytes > expected_size {
            return Ok(self.fail_active_download(RemoteDesktopFileTransferFailureKind::Remote));
        }
        if current.temporary_file.write_all(&bytes).is_err() {
            return Ok(self.fail_active_download(RemoteDesktopFileTransferFailureKind::Local));
        }
        current.received_bytes = next_file_bytes;
        batch.transferred_bytes = batch
            .transferred_bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| "VNC file download total size overflowed.".to_string())?;

        let should_report = batch
            .transferred_bytes
            .saturating_sub(batch.last_reported_bytes)
            >= VNC_DOWNLOAD_PROGRESS_INTERVAL_BYTES
            || current.received_bytes == expected_size;
        let mut actions = VncVendorFileActions::default();
        if should_report {
            batch.last_reported_bytes = batch.transferred_bytes;
            actions
                .events
                .push(RemoteDesktopHelperEvent::VncFileTransferProgress {
                    transfer_id: batch.transfer_id.clone(),
                    file_name: current.plan.entry.name.clone(),
                    transferred_bytes: batch.transferred_bytes,
                    total_bytes: batch.total_bytes,
                    completed_files: batch.completed_files,
                    total_files: batch.total_files,
                });
        }
        Ok(actions)
    }

    fn finish_download_file(
        &mut self,
        modified_seconds: u32,
    ) -> Result<VncVendorFileActions, String> {
        let Some(mut batch) = self.active_download.take() else {
            return Err("VNC server ended a download without an active transfer.".to_string());
        };
        let Some(current) = batch.current.take() else {
            return Err("VNC download transfer has no active file.".to_string());
        };
        let expected_size = current.plan.entry.size.unwrap_or(current.received_bytes);
        if current.received_bytes != expected_size {
            self.active_download = Some(batch);
            return Ok(self.fail_active_download(RemoteDesktopFileTransferFailureKind::Remote));
        }

        let persisted = persist_download_file(current, batch.conflict_policy, modified_seconds);
        match persisted {
            Ok(Some(path)) => batch.completed_paths.push(path),
            Ok(None) => batch.skipped_files = batch.skipped_files.saturating_add(1),
            Err(_) => {
                self.active_download = Some(batch);
                return Ok(self.fail_active_download(RemoteDesktopFileTransferFailureKind::Local));
            }
        }
        batch.completed_files = batch.completed_files.saturating_add(1);
        let mut actions = VncVendorFileActions::default();
        if let Some(message) = batch.start_next()? {
            actions.messages.push(message);
            self.active_download = Some(batch);
        } else {
            actions
                .events
                .push(RemoteDesktopHelperEvent::VncFileTransferCompleted {
                    transfer_id: batch.transfer_id,
                    paths: batch.completed_paths,
                    skipped_files: batch.skipped_files,
                });
        }
        Ok(actions)
    }

    fn read_download_failed(
        &mut self,
        reader: &mut impl Read,
    ) -> Result<VncVendorFileActions, String> {
        let _reason = read_file_failure_reason(reader, "download failure")?;
        if self.canceling_download.take().is_some() {
            return Ok(VncVendorFileActions::default());
        }
        let Some(batch) = self.active_download.take() else {
            return Err("VNC server rejected a download that is not active.".to_string());
        };
        Ok(VncVendorFileActions {
            messages: Vec::new(),
            events: vec![RemoteDesktopHelperEvent::VncFileTransferFailed {
                transfer_id: batch.transfer_id,
                kind: RemoteDesktopFileTransferFailureKind::Remote,
            }],
        })
    }

    fn fail_active_download(
        &mut self,
        kind: RemoteDesktopFileTransferFailureKind,
    ) -> VncVendorFileActions {
        let Some(batch) = self.active_download.take() else {
            return VncVendorFileActions::default();
        };
        self.canceling_download = Some(batch.transfer_id.clone());
        VncVendorFileActions {
            messages: vec![file_failure_message(
                FILE_DOWNLOAD_CANCEL.code as u8,
                b"Transfer stopped.",
            )],
            events: vec![RemoteDesktopHelperEvent::VncFileTransferFailed {
                transfer_id: batch.transfer_id,
                kind,
            }],
        }
    }

    fn read_upload_cancel(
        &mut self,
        reader: &mut impl Read,
    ) -> Result<VncVendorFileActions, String> {
        let message = read_file_failure_reason(reader, "upload cancel")?;
        let Some(transfer_id) = self.active_upload.take() else {
            return Err("VNC server canceled an upload that is not active.".to_string());
        };
        Ok(VncVendorFileActions {
            messages: Vec::new(),
            events: vec![RemoteDesktopHelperEvent::ClipboardTransferFailed {
                transfer_id,
                message,
            }],
        })
    }
}

fn encode_file_list_request(path: &str) -> Result<Vec<u8>, String> {
    let path_size = u16::try_from(path.len())
        .map_err(|_| "VNC remote directory path is too long.".to_string())?;
    let mut message = Vec::with_capacity(4 + path.len());
    message.push(FILE_LIST_REQUEST.code as u8);
    // TightVNC uses the drives flag with an empty path to discover the server
    // roots before the client knows whether its path separator is Unix or DOS.
    message.push(if path.is_empty() { 0x10 } else { 0 });
    push_be_u16(&mut message, path_size);
    message.extend_from_slice(path.as_bytes());
    Ok(message)
}

fn encode_download_request(path: &str) -> Result<Vec<u8>, String> {
    validate_remote_path(path)?;
    let path_size =
        u16::try_from(path.len()).map_err(|_| "VNC remote file path is too long.".to_string())?;
    let mut message = Vec::with_capacity(8 + path.len());
    message.push(FILE_DOWNLOAD_REQUEST.code as u8);
    message.push(0);
    push_be_u16(&mut message, path_size);
    push_be_u32(&mut message, 0);
    message.extend_from_slice(path.as_bytes());
    Ok(message)
}

fn decode_file_list_names(wire: &[u8], expected_size: usize) -> Result<Vec<u8>, String> {
    if wire.len() == expected_size {
        return Ok(wire.to_vec());
    }
    decode_zlib_exact(wire, expected_size, "VNC file list names")
}

fn decode_zlib_exact(wire: &[u8], expected_size: usize, label: &str) -> Result<Vec<u8>, String> {
    let limit = u64::try_from(expected_size)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut decoder = ZlibDecoder::new(wire);
    let mut decoded = Vec::with_capacity(expected_size);
    decoder
        .by_ref()
        .take(limit)
        .read_to_end(&mut decoded)
        .map_err(|error| format!("{label} decompression failed: {error}"))?;
    if decoded.len() != expected_size {
        return Err(format!("{label} decompressed to an unexpected size."));
    }
    Ok(decoded)
}

fn persist_download_file(
    current: ActiveDownloadFile,
    conflict_policy: RemoteDesktopFileConflictPolicy,
    modified_seconds: u32,
) -> Result<Option<PathBuf>, String> {
    let ActiveDownloadFile {
        plan,
        temporary_file,
        ..
    } = current;
    let modified = SystemTime::UNIX_EPOCH
        .checked_add(Duration::from_secs(u64::from(modified_seconds)))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    temporary_file
        .as_file()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .map_err(|error| format!("VNC download timestamp update failed: {error}"))?;
    temporary_file
        .as_file()
        .sync_all()
        .map_err(|error| format!("VNC download temporary file sync failed: {error}"))?;

    match conflict_policy {
        RemoteDesktopFileConflictPolicy::Overwrite => {
            let (file, temporary_path) = temporary_file.keep().map_err(|error| {
                format!("VNC download temporary file retention failed: {error}")
            })?;
            drop(file);
            if let Err(error) = durable_replace(&temporary_path, &plan.target_path) {
                let _ = fs::remove_file(&temporary_path);
                return Err(format!("VNC download replacement failed: {error}"));
            }
            Ok(Some(plan.target_path))
        }
        RemoteDesktopFileConflictPolicy::Skip => {
            match temporary_file.persist_noclobber(&plan.target_path) {
                Ok(file) => {
                    drop(file);
                    Ok(Some(plan.target_path))
                }
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
                Err(error) => Err(format!("VNC download persistence failed: {}", error.error)),
            }
        }
        RemoteDesktopFileConflictPolicy::Rename => {
            persist_download_with_unique_name(temporary_file, &plan.target_path).map(Some)
        }
    }
}

fn persist_download_with_unique_name(
    mut temporary_file: NamedTempFile,
    requested_path: &Path,
) -> Result<PathBuf, String> {
    let mut reserved = HashSet::new();
    for _ in 0..10_000 {
        let candidate = unique_download_target(requested_path, &reserved)?;
        match temporary_file.persist_noclobber(&candidate) {
            Ok(file) => {
                drop(file);
                return Ok(candidate);
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                reserved.insert(candidate);
                temporary_file = error.file;
            }
            Err(error) => {
                return Err(format!("VNC download persistence failed: {}", error.error));
            }
        }
    }
    Err("VNC download could not allocate a unique local name.".to_string())
}

fn unique_download_target(
    requested_path: &Path,
    reserved: &HashSet<PathBuf>,
) -> Result<PathBuf, String> {
    if !requested_path.exists() && !reserved.contains(requested_path) {
        return Ok(requested_path.to_path_buf());
    }
    let file_name = requested_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "VNC download target name is not valid UTF-8.".to_string())?;
    let (stem, extension) = split_download_name(file_name);
    let parent = requested_path
        .parent()
        .ok_or_else(|| "VNC download target has no parent directory.".to_string())?;
    for index in 1..10_000 {
        let candidate_name = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = parent.join(candidate_name);
        if !candidate.exists() && !reserved.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err("VNC download could not allocate a unique local name.".to_string())
}

fn split_download_name(name: &str) -> (&str, Option<&str>) {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => {
            (stem, Some(extension))
        }
        _ => (name, None),
    }
}

fn validate_remote_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > MAX_VNC_REMOTE_PATH_BYTES
        || path
            .chars()
            .any(|character| character == '\0' || character.is_control())
    {
        return Err("VNC remote path is unsafe or too long.".to_string());
    }
    Ok(())
}

fn remote_child_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        return name.to_string();
    }
    if parent.ends_with('/') || parent.ends_with('\\') {
        return format!("{parent}{name}");
    }
    let separator = if parent.contains('\\')
        || parent
            .as_bytes()
            .get(1)
            .is_some_and(|character| *character == b':')
    {
        '\\'
    } else {
        '/'
    };
    format!("{parent}{separator}{name}")
}

fn read_file_failure_reason(reader: &mut impl Read, message_kind: &str) -> Result<String, String> {
    let header = read_exact_array::<3, _>(reader)
        .map_err(|error| format!("VNC file {message_kind} header read failed: {error}"))?;
    let reason_size = usize::from(be_u16(&header[1..3]));
    if reason_size > 4096 {
        return Err("VNC file failure reason exceeds the helper limit.".to_string());
    }
    let reason = read_exact_vec(reader, reason_size)
        .map_err(|error| format!("VNC file {message_kind} reason read failed: {error}"))?;
    Ok(String::from_utf8_lossy(&reason).into_owned())
}

fn file_failure_message(message_type: u8, reason: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(4 + reason.len());
    message.push(message_type);
    message.push(0);
    push_be_u16(&mut message, reason.len() as u16);
    message.extend_from_slice(reason);
    message
}

struct ValidatedUploadFile {
    path: PathBuf,
    remote_name: Vec<u8>,
    size: u64,
    modified_seconds: u32,
}

fn validate_local_upload_file(path: &Path) -> Result<ValidatedUploadFile, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("VNC upload source metadata failed: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("VNC file upload rejects symbolic links.".to_string());
    }
    if !metadata.is_file() {
        return Err("VNC file upload accepts ordinary files only.".to_string());
    }
    if metadata.len() > MAX_VNC_FILE_BYTES {
        return Err("VNC file upload exceeds the 20 MiB file limit.".to_string());
    }
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("VNC upload source resolution failed: {error}"))?;
    let name = canonical
        .file_name()
        .ok_or_else(|| "VNC upload source has no file name.".to_string())?
        .to_str()
        .ok_or_else(|| "VNC upload file name is not valid UTF-8.".to_string())?;
    validate_remote_file_name(name)?;
    let remote_name = name.as_bytes().to_vec();
    let modified_seconds = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs().min(u64::from(u32::MAX)) as u32)
        .unwrap_or(0);
    Ok(ValidatedUploadFile {
        path: canonical,
        remote_name,
        size: metadata.len(),
        modified_seconds,
    })
}

fn encode_tight_upload_file(file: &ValidatedUploadFile) -> Result<Vec<Vec<u8>>, String> {
    let name_size = u16::try_from(file.remote_name.len())
        .map_err(|_| "VNC upload file name is too long.".to_string())?;
    let mut request = Vec::with_capacity(8 + file.remote_name.len());
    request.push(FILE_UPLOAD_REQUEST.code as u8);
    request.push(0);
    push_be_u16(&mut request, name_size);
    push_be_u32(&mut request, 0);
    request.extend_from_slice(&file.remote_name);

    let estimated_chunks =
        usize::try_from(file.size.div_ceil(VNC_FILE_CHUNK_BYTES as u64)).unwrap_or(0);
    let mut messages = Vec::with_capacity(estimated_chunks.saturating_add(2));
    messages.push(request);
    let mut source = File::open(&file.path)
        .map_err(|error| format!("VNC upload source open failed: {error}"))?;
    let mut buffer = vec![0; VNC_FILE_CHUNK_BYTES];
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|error| format!("VNC upload source read failed: {error}"))?;
        if read == 0 {
            break;
        }
        let chunk_size = u16::try_from(read)
            .map_err(|_| "VNC upload chunk exceeds the protocol limit.".to_string())?;
        let mut chunk = Vec::with_capacity(6 + read);
        chunk.push(FILE_UPLOAD_DATA.code as u8);
        chunk.push(0);
        push_be_u16(&mut chunk, chunk_size);
        push_be_u16(&mut chunk, chunk_size);
        chunk.extend_from_slice(&buffer[..read]);
        messages.push(chunk);
    }
    let mut end = vec![FILE_UPLOAD_DATA.code as u8, 0, 0, 0, 0, 0];
    push_be_u32(&mut end, file.modified_seconds);
    messages.push(end);
    Ok(messages)
}

fn validate_remote_file_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.as_bytes().len() > MAX_VNC_FILE_NAME_BYTES
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
        || matches!(name, "." | "..")
    {
        return Err("VNC file name is unsafe or too long.".to_string());
    }
    Ok(())
}

pub(super) const fn tight_vendor() -> [u8; 4] {
    TIGHT_VENDOR
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_file_capabilities() -> TightInteractionCapabilities {
        TightInteractionCapabilities {
            server_messages: vec![
                FILE_LIST_DATA,
                FILE_DOWNLOAD_DATA,
                FILE_UPLOAD_CANCEL,
                FILE_DOWNLOAD_FAILED,
            ],
            client_messages: vec![
                FILE_LIST_REQUEST,
                FILE_DOWNLOAD_REQUEST,
                FILE_UPLOAD_REQUEST,
                FILE_UPLOAD_DATA,
                FILE_DOWNLOAD_CANCEL,
                FILE_UPLOAD_FAILED,
            ],
            encodings: Vec::new(),
        }
    }

    #[test]
    fn vendor_files_require_every_registered_capability_signature() {
        let capabilities = TightFileCapabilities::from_interaction(&full_file_capabilities());
        assert!(capabilities.list && capabilities.download && capabilities.upload);

        let mut forged = full_file_capabilities();
        forged.client_messages[0].vendor = *b"FAKE";
        let capabilities = TightFileCapabilities::from_interaction(&forged);
        assert!(!capabilities.list);
        assert!(capabilities.download && capabilities.upload);
    }

    #[test]
    fn tight_interaction_caps_preserve_code_vendor_and_signature() {
        let capabilities = full_file_capabilities();
        let mut bytes = Vec::new();
        push_be_u16(&mut bytes, capabilities.server_messages.len() as u16);
        push_be_u16(&mut bytes, capabilities.client_messages.len() as u16);
        push_be_u16(&mut bytes, 0);
        push_be_u16(&mut bytes, 0);
        for capability in capabilities
            .server_messages
            .iter()
            .chain(capabilities.client_messages.iter())
        {
            push_be_i32(&mut bytes, capability.code);
            bytes.extend_from_slice(&capability.vendor);
            bytes.extend_from_slice(&capability.signature);
        }

        let parsed = read_tight_interaction_capabilities(&mut bytes.as_slice()).unwrap();
        assert_eq!(parsed, capabilities);
    }

    #[test]
    fn upload_rejects_directories_and_encodes_bounded_regular_files() {
        let directory = tempfile::tempdir().unwrap();
        let file_path = directory.path().join("ordinary.txt");
        fs::write(&file_path, b"bounded content").unwrap();
        let mut session = VncVendorFileSession::new(TightFileCapabilities::from_interaction(
            &full_file_capabilities(),
        ));

        assert!(
            session
                .upload_payload("directory", &[directory.path().to_path_buf()])
                .is_err()
        );
        let payload = session
            .upload_payload("file", std::slice::from_ref(&file_path))
            .unwrap();
        assert_eq!(payload[0], FILE_UPLOAD_REQUEST.code as u8);
        let end = &payload[payload.len() - 10..];
        assert_eq!(end[0], FILE_UPLOAD_DATA.code as u8);
        assert_eq!(&end[2..6], &[0, 0, 0, 0]);
    }

    #[cfg(unix)]
    #[test]
    fn upload_rejects_symbolic_links() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let file_path = directory.path().join("ordinary.txt");
        let link_path = directory.path().join("link.txt");
        fs::write(&file_path, b"content").unwrap();
        symlink(&file_path, &link_path).unwrap();
        let mut session = VncVendorFileSession::new(TightFileCapabilities::from_interaction(
            &full_file_capabilities(),
        ));

        assert!(session.upload_payload("symlink", &[link_path]).is_err());
    }

    #[test]
    fn upload_larger_than_writer_queue_is_one_atomic_payload() {
        let directory = tempfile::tempdir().unwrap();
        let file_path = directory.path().join("many-chunks.bin");
        fs::write(
            &file_path,
            vec![0x5a; VNC_FILE_CHUNK_BYTES * (VNC_IO_COMMAND_CAPACITY + 1)],
        )
        .unwrap();
        let mut session = VncVendorFileSession::new(TightFileCapabilities::from_interaction(
            &full_file_capabilities(),
        ));

        let payload = session
            .upload_payload("atomic", std::slice::from_ref(&file_path))
            .unwrap();

        assert!(payload.len() > VNC_FILE_CHUNK_BYTES * VNC_IO_COMMAND_CAPACITY);
        assert_eq!(payload[0], FILE_UPLOAD_REQUEST.code as u8);
    }

    #[test]
    fn file_list_response_establishes_downloadable_remote_paths() {
        let mut session = VncVendorFileSession::new(TightFileCapabilities::from_interaction(
            &full_file_capabilities(),
        ));
        let request = session
            .request_list("roots".to_string(), String::new())
            .unwrap();
        assert_eq!(&request[..4], &[FILE_LIST_REQUEST.code as u8, 0x10, 0, 0]);

        let names = b"folder\0report.txt\0";
        let mut response = Vec::new();
        response.push(0x10);
        push_be_u16(&mut response, 2);
        push_be_u16(&mut response, names.len() as u16);
        push_be_u16(&mut response, names.len() as u16);
        push_be_u32(&mut response, VNC_DIRECTORY_SIZE_MARKER);
        push_be_u32(&mut response, 0);
        push_be_u32(&mut response, 5);
        push_be_u32(&mut response, 42);
        response.extend_from_slice(names);

        let actions = session
            .observe_server_message(FILE_LIST_DATA.code as u8, &mut response.as_slice())
            .unwrap();
        assert!(matches!(
            actions.events.as_slice(),
            [RemoteDesktopHelperEvent::VncRemoteFilesListed { path, entries, .. }]
                if path.is_empty()
                    && entries[0].kind == RemoteDesktopRemoteFileKind::Directory
                    && entries[1].path == "report.txt"
        ));
    }

    #[test]
    fn download_stream_uses_atomic_conflict_rename_and_reports_completion() {
        let destination = tempfile::tempdir().unwrap();
        fs::write(destination.path().join("report.txt"), b"existing").unwrap();
        let mut session = VncVendorFileSession::new(TightFileCapabilities::from_interaction(
            &full_file_capabilities(),
        ));
        session.listings.insert(
            "/tmp".to_string(),
            vec![RemoteDesktopRemoteFileEntry {
                name: "report.txt".to_string(),
                path: "/tmp/report.txt".to_string(),
                kind: RemoteDesktopRemoteFileKind::File,
                size: Some(5),
                modified_seconds: Some(0),
            }],
        );

        let start = session
            .start_download(
                "download".to_string(),
                vec!["/tmp/report.txt".to_string()],
                destination.path().to_path_buf(),
                RemoteDesktopFileConflictPolicy::Rename,
            )
            .unwrap();
        assert_eq!(start.messages[0][0], FILE_DOWNLOAD_REQUEST.code as u8);

        let mut chunk = vec![0, 0, 5, 0, 5];
        chunk.extend_from_slice(b"hello");
        session
            .observe_server_message(FILE_DOWNLOAD_DATA.code as u8, &mut chunk.as_slice())
            .unwrap();
        let mut end = vec![0, 0, 0, 0, 0];
        end.extend_from_slice(&42u32.to_le_bytes());
        let completed = session
            .observe_server_message(FILE_DOWNLOAD_DATA.code as u8, &mut end.as_slice())
            .unwrap();

        let renamed = fs::canonicalize(destination.path())
            .unwrap()
            .join("report (1).txt");
        assert_eq!(fs::read(&renamed).unwrap(), b"hello");
        let [RemoteDesktopHelperEvent::VncFileTransferCompleted { paths, .. }] =
            completed.events.as_slice()
        else {
            panic!("download did not emit one completion event");
        };
        assert_eq!(paths, std::slice::from_ref(&renamed));
    }

    #[test]
    fn canceled_download_drains_buffered_server_blocks_without_disconnect() {
        let destination = tempfile::tempdir().unwrap();
        let mut session = VncVendorFileSession::new(TightFileCapabilities::from_interaction(
            &full_file_capabilities(),
        ));
        session.listings.insert(
            "/tmp".to_string(),
            vec![RemoteDesktopRemoteFileEntry {
                name: "report.txt".to_string(),
                path: "/tmp/report.txt".to_string(),
                kind: RemoteDesktopRemoteFileKind::File,
                size: Some(5),
                modified_seconds: Some(0),
            }],
        );
        session
            .start_download(
                "download".to_string(),
                vec!["/tmp/report.txt".to_string()],
                destination.path().to_path_buf(),
                RemoteDesktopFileConflictPolicy::Rename,
            )
            .unwrap();

        let canceled = session.cancel_download("download".to_string());
        assert_eq!(canceled.messages[0][0], FILE_DOWNLOAD_CANCEL.code as u8);
        let mut buffered_chunk = vec![0, 0, 5, 0, 5];
        buffered_chunk.extend_from_slice(b"hello");
        session
            .observe_server_message(
                FILE_DOWNLOAD_DATA.code as u8,
                &mut buffered_chunk.as_slice(),
            )
            .unwrap();
        let mut end = vec![0, 0, 0, 0, 0];
        end.extend_from_slice(&0u32.to_le_bytes());
        session
            .observe_server_message(FILE_DOWNLOAD_DATA.code as u8, &mut end.as_slice())
            .unwrap();
        assert!(session.canceling_download.is_none());
    }
}
