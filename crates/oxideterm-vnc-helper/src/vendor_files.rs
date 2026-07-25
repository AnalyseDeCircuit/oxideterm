// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use super::*;

const TIGHT_VENDOR: [u8; 4] = *b"TGHT";
const MAX_TIGHT_CAPABILITIES: usize = 256;
const MAX_VNC_FILE_COUNT: usize = 128;
const MAX_VNC_FILE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_VNC_FILE_NAME_BYTES: usize = 255;
const VNC_FILE_CHUNK_BYTES: usize = 60 * 1024;

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
pub(super) struct VncVendorFileSession {
    capabilities: TightFileCapabilities,
    canceled_transfers: std::collections::HashSet<String>,
    active_upload: Option<String>,
}

impl VncVendorFileSession {
    pub(super) fn new(capabilities: TightFileCapabilities) -> Self {
        Self {
            capabilities,
            canceled_transfers: std::collections::HashSet::new(),
            active_upload: None,
        }
    }

    pub(super) fn cancel(&mut self, transfer_id: String) -> Option<Vec<u8>> {
        const REASON: &[u8] = b"Canceled.";
        if self.active_upload.as_deref() == Some(transfer_id.as_str()) {
            self.active_upload = None;
            return Some(file_failure_message(FILE_UPLOAD_FAILED.code as u8, REASON));
        }
        self.canceled_transfers.insert(transfer_id);
        None
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
    ) -> Result<Vec<RemoteDesktopHelperEvent>, String> {
        match message_type {
            132 if self.capabilities.upload => self.read_upload_cancel(reader),
            _ => Err("VNC server sent an unnegotiated Tight file-transfer message.".to_string()),
        }
    }

    fn read_upload_cancel(
        &mut self,
        reader: &mut impl Read,
    ) -> Result<Vec<RemoteDesktopHelperEvent>, String> {
        let message = read_file_failure_reason(reader, "upload cancel")?;
        let Some(transfer_id) = self.active_upload.take() else {
            return Err("VNC server canceled an upload that is not active.".to_string());
        };
        Ok(vec![RemoteDesktopHelperEvent::ClipboardTransferFailed {
            transfer_id,
            message,
        }])
    }
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
}
