// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use std::{
    collections::{BTreeMap, HashSet},
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write as _},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

const RDP_CLIPBOARD_MAX_FILE_COUNT: usize = 1_024;
const RDP_CLIPBOARD_MAX_FILE_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const RDP_CLIPBOARD_MAX_TRANSFER_SIZE: u64 = 4 * 1024 * 1024 * 1024;
const RDP_CLIPBOARD_MAX_REQUEST_BYTES: u32 = 8 * 1024 * 1024;
const RDP_CLIPBOARD_DOWNLOAD_CHUNK_BYTES: u32 = 64 * 1024;
const RDP_CLIPBOARD_STAGING_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug)]
enum RemoteClipboardDownloadPhase {
    Size {
        stream_id: u32,
    },
    Data {
        stream_id: u32,
        file_size: u64,
        position: u64,
        requested_size: u32,
    },
}

#[derive(Debug)]
struct RemoteClipboardDownload {
    transfer_id: String,
    directory: PathBuf,
    descriptors: Vec<FileDescriptor>,
    file_indices: Vec<usize>,
    clip_data_id: Option<u32>,
    file_cursor: usize,
    completed_paths: Vec<PathBuf>,
    completed_bytes: u64,
    phase: RemoteClipboardDownloadPhase,
    next_stream_id: u32,
}

#[derive(Clone)]
struct LocalClipboardFile {
    path: PathBuf,
    name: String,
    relative_path: Option<String>,
    size: u64,
    directory: bool,
}

impl fmt::Debug for LocalClipboardFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalClipboardFile")
            .field("name", &"<redacted>")
            .field("size", &self.size)
            .field("directory", &self.directory)
            .finish()
    }
}

pub(super) struct ClientClipboardBackend {
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_tx: ClientRdpOutputSender,
    options: oxideterm_remote_desktop::RemoteDesktopClipboardOptions,
    local_text: Option<String>,
    local_data: Option<RemoteDesktopClipboardData>,
    remote_text_format: Option<ClipboardFormatId>,
    remote_data_format: Option<RdpClipboardDataFormat>,
    local_files: Option<Arc<Vec<LocalClipboardFile>>>,
    local_file_transfer_id: Option<String>,
    locked_local_files: BTreeMap<u32, Arc<Vec<LocalClipboardFile>>>,
    temporary_directory: String,
    remote_download: Option<RemoteClipboardDownload>,
}

impl fmt::Debug for ClientClipboardBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientClipboardBackend")
            .field("has_local_text", &self.local_text.is_some())
            .field("has_local_data", &self.local_data.is_some())
            .field("remote_text_format", &self.remote_text_format)
            .field("remote_data_format", &self.remote_data_format)
            .finish()
    }
}

impl Drop for ClientClipboardBackend {
    fn drop(&mut self) {
        if let Some(download) = self.remote_download.take() {
            // Incomplete transfers must not leave partially written clipboard files behind.
            cleanup_remote_clipboard_download(&download);
        }
    }
}

impl_as_any!(ClientClipboardBackend);

impl ClientClipboardBackend {
    pub(super) fn new(
        input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
        output_tx: ClientRdpOutputSender,
        options: oxideterm_remote_desktop::RemoteDesktopClipboardOptions,
    ) -> Self {
        let temporary_directory = remote_clipboard_session_directory();
        Self {
            input_tx,
            output_tx,
            options,
            local_text: None,
            local_data: None,
            remote_text_format: None,
            remote_data_format: None,
            local_files: None,
            local_file_transfer_id: None,
            locked_local_files: BTreeMap::new(),
            temporary_directory: temporary_directory.to_string_lossy().into_owned(),
            remote_download: None,
        }
    }

    pub(super) fn set_local_text(&mut self, text: String) {
        if !self.options.text {
            return;
        }
        self.local_text = Some(text);
        self.local_data = None;
        self.local_files = None;
        self.local_file_transfer_id = None;
    }

    pub(super) fn set_local_data(&mut self, data: RemoteDesktopClipboardData) {
        if !self.options.images {
            return;
        }
        self.local_text = None;
        self.local_data = Some(data);
        self.local_files = None;
        self.local_file_transfer_id = None;
    }

    fn set_local_files(
        &mut self,
        transfer_id: String,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<FileDescriptor>, String> {
        if !self.options.files {
            return Err("RDP clipboard file redirection is disabled.".to_string());
        }
        let files = collect_local_clipboard_files(paths)?;

        let descriptors = files
            .iter()
            .map(|file| {
                let attributes = if file.directory {
                    ClipboardFileAttributes::DIRECTORY
                } else {
                    ClipboardFileAttributes::NORMAL
                };
                let mut descriptor = FileDescriptor::new(file.name.clone())
                    .with_attributes(attributes)
                    .with_file_size(file.size);
                if let Some(relative_path) = file.relative_path.as_ref() {
                    descriptor = descriptor.with_relative_path(relative_path.clone());
                }
                descriptor
            })
            .collect();
        self.local_text = None;
        self.local_data = None;
        self.local_files = Some(Arc::new(files));
        self.local_file_transfer_id = Some(transfer_id);
        Ok(descriptors)
    }

    fn report_file_transfer_failure(&self, transfer_id: String, message: String) {
        let _ = self.output_tx.send_control(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::ClipboardTransferFailed {
                transfer_id,
                message,
            },
        ));
    }

    fn send_clipboard_message(&self, message: ClipboardMessage) {
        let _ = self.input_tx.send(RdpInputEvent::Clipboard(message));
    }

    fn send_local_format_list(&self) {
        let formats = if let Some(data) = self.local_data.as_ref() {
            image_clipboard_formats(data.format)
        } else if self.local_text.is_some() {
            text_clipboard_formats()
        } else {
            Vec::new()
        };
        self.send_clipboard_message(ClipboardMessage::SendInitiateCopy(formats));
    }
}

impl CliprdrBackend for ClientClipboardBackend {
    fn temporary_directory(&self) -> &str {
        &self.temporary_directory
    }

    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        if self.options.files {
            ClipboardGeneralCapabilityFlags::STREAM_FILECLIP_ENABLED
                | ClipboardGeneralCapabilityFlags::FILECLIP_NO_FILE_PATHS
                | ClipboardGeneralCapabilityFlags::CAN_LOCK_CLIPDATA
        } else {
            ClipboardGeneralCapabilityFlags::empty()
        }
    }

    fn on_ready(&mut self) {
        // CLIPRDR may become ready after the UI has already supplied local
        // clipboard text. Advertise the cached formats once the channel is
        // usable so the server can request that text immediately.
        self.send_local_format_list();
    }

    fn on_request_format_list(&mut self) {
        // The CLIPRDR initialization sequence requires the client to advertise
        // its current clipboard formats, even when the list is empty.
        self.send_local_format_list();
    }

    fn on_process_negotiated_capabilities(
        &mut self,
        _capabilities: ClipboardGeneralCapabilityFlags,
    ) {
    }

    fn on_remote_copy(&mut self, available_formats: &[ClipboardFormat]) {
        self.remote_text_format = None;
        self.remote_data_format = None;

        if self.options.files
            && let Some(format) = available_formats.iter().find(|format| {
                format
                    .name()
                    .is_some_and(|name| name.value() == ClipboardFormatName::FILE_LIST.value())
            })
        {
            self.send_clipboard_message(ClipboardMessage::SendInitiatePaste(format.id));
            return;
        }

        if self.options.images
            && let Some(format) = preferred_image_clipboard_format(available_formats)
        {
            self.remote_data_format = Some(format);
            self.send_clipboard_message(ClipboardMessage::SendInitiatePaste(format.id));
            return;
        }

        if self.options.text
            && let Some(format) = preferred_text_clipboard_format(available_formats)
        {
            self.remote_text_format = Some(format);
            self.send_clipboard_message(ClipboardMessage::SendInitiatePaste(format));
        }
    }

    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        let response = if let Some(data) = self.local_data.as_ref() {
            if let Some(bytes) = encode_local_clipboard_data(data, request.format) {
                FormatDataResponse::new_data(bytes).into_owned()
            } else {
                FormatDataResponse::new_error().into_owned()
            }
        } else {
            match (request.format, self.local_text.as_deref()) {
                (ClipboardFormatId::CF_UNICODETEXT, Some(text)) => {
                    FormatDataResponse::new_unicode_string(text).into_owned()
                }
                (ClipboardFormatId::CF_TEXT, Some(text)) => {
                    FormatDataResponse::new_string(text).into_owned()
                }
                _ => FormatDataResponse::new_error().into_owned(),
            }
        };
        self.send_clipboard_message(ClipboardMessage::SendFormatData(response));
    }

    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        if response.is_error() {
            return;
        }

        if let Some(format) = self.remote_data_format.take() {
            let data = decode_remote_clipboard_data(format, response.data().to_vec());
            if let Some(data) = data {
                let _ = self.output_tx.send_control(ClientRdpOutput::Event(
                    RemoteDesktopHelperEvent::ClipboardData { data },
                ));
            }
            return;
        }

        let text = match self.remote_text_format.take() {
            Some(ClipboardFormatId::CF_UNICODETEXT) => response.to_unicode_string().ok(),
            Some(ClipboardFormatId::CF_TEXT) => response.to_string().ok(),
            _ => response
                .to_unicode_string()
                .or_else(|_| response.to_string())
                .ok(),
        };
        if let Some(text) = text {
            let _ = self.output_tx.send_control(ClientRdpOutput::Event(
                RemoteDesktopHelperEvent::ClipboardText { text },
            ));
        }
    }

    fn on_file_contents_request(&mut self, request: FileContentsRequest) {
        let response = self.local_file_contents_response(&request);
        self.send_clipboard_message(ClipboardMessage::SendFileContentsResponse(response));
    }

    fn on_file_contents_response(&mut self, response: FileContentsResponse<'_>) {
        self.handle_remote_file_contents_response(response);
    }

    fn on_lock(&mut self, data_id: LockDataId) {
        if let Some(files) = self.local_files.clone() {
            self.locked_local_files.insert(data_id.0, files);
        }
    }

    fn on_unlock(&mut self, data_id: LockDataId) {
        self.locked_local_files.remove(&data_id.0);
    }

    fn on_remote_file_list(&mut self, files: &[FileDescriptor], clip_data_id: Option<u32>) {
        self.start_remote_file_download(files, clip_data_id);
    }
}

impl ClientClipboardBackend {
    fn local_file_contents_response(
        &self,
        request: &FileContentsRequest,
    ) -> FileContentsResponse<'static> {
        let files = match request.data_id {
            Some(data_id) => self.locked_local_files.get(&data_id),
            None => self.local_files.as_ref(),
        };
        let Some(files) = files else {
            return FileContentsResponse::new_error(request.stream_id);
        };
        let Ok(index) = usize::try_from(request.index) else {
            return FileContentsResponse::new_error(request.stream_id);
        };
        let Some(file) = files.get(index) else {
            return FileContentsResponse::new_error(request.stream_id);
        };
        if request.flags.validate().is_err() {
            return FileContentsResponse::new_error(request.stream_id);
        }
        if request.flags == FileContentsFlags::SIZE {
            if request.position != 0 || request.requested_size != 8 {
                return FileContentsResponse::new_error(request.stream_id);
            }
            return FileContentsResponse::new_size_response(request.stream_id, file.size)
                .into_owned();
        }
        if file.directory
            || request.flags != FileContentsFlags::RANGE
            || request.requested_size > RDP_CLIPBOARD_MAX_REQUEST_BYTES
            || request.position > file.size
        {
            return FileContentsResponse::new_error(request.stream_id);
        }

        let remaining = file.size.saturating_sub(request.position);
        let requested = u64::from(request.requested_size).min(remaining);
        let Ok(requested) = usize::try_from(requested) else {
            return FileContentsResponse::new_error(request.stream_id);
        };
        let mut bytes = vec![0_u8; requested];
        let read_result = File::open(&file.path).and_then(|mut source| {
            source.seek(SeekFrom::Start(request.position))?;
            source.read_exact(&mut bytes)
        });
        if read_result.is_err() {
            return FileContentsResponse::new_error(request.stream_id);
        }
        FileContentsResponse::new_data_response(request.stream_id, bytes).into_owned()
    }

    fn start_remote_file_download(&mut self, files: &[FileDescriptor], clip_data_id: Option<u32>) {
        if let Some(previous_download) = self.remote_download.take() {
            cleanup_remote_clipboard_download(&previous_download);
        }
        let transfer_id = uuid::Uuid::new_v4().to_string();
        let result = prepare_remote_clipboard_download(
            PathBuf::from(&self.temporary_directory),
            transfer_id.clone(),
            files,
            clip_data_id,
        );
        match result {
            Ok(RemoteClipboardDownloadStart::Request(download, request)) => {
                self.remote_download = Some(download);
                self.send_clipboard_message(ClipboardMessage::SendFileContentsRequest(request));
            }
            Ok(RemoteClipboardDownloadStart::Complete(paths)) => {
                let _ = self.output_tx.send_control(ClientRdpOutput::Event(
                    RemoteDesktopHelperEvent::ClipboardFilesReady { transfer_id, paths },
                ));
            }
            Err(message) => self.report_file_transfer_failure(transfer_id, message),
        }
    }

    fn cancel_transfer(&mut self, transfer_id: &str) {
        if self.local_file_transfer_id.as_deref() == Some(transfer_id) {
            self.local_file_transfer_id = None;
            self.local_files = None;
            self.locked_local_files.clear();
        }
        if self
            .remote_download
            .as_ref()
            .is_some_and(|download| download.transfer_id == transfer_id)
            && let Some(download) = self.remote_download.take()
        {
            cleanup_remote_clipboard_download(&download);
        }
    }

    fn handle_remote_file_contents_response(&mut self, response: FileContentsResponse<'_>) {
        let Some(mut download) = self.remote_download.take() else {
            return;
        };
        let transfer_id = download.transfer_id.clone();
        let outcome = advance_remote_clipboard_download(&mut download, response);
        match outcome {
            Ok(RemoteClipboardDownloadAdvance::Request(request)) => {
                self.remote_download = Some(download);
                self.send_clipboard_message(ClipboardMessage::SendFileContentsRequest(request));
            }
            Ok(RemoteClipboardDownloadAdvance::Complete(paths)) => {
                let _ = self.output_tx.send_control(ClientRdpOutput::Event(
                    RemoteDesktopHelperEvent::ClipboardFilesReady { transfer_id, paths },
                ));
            }
            Err(message) => {
                cleanup_remote_clipboard_download(&download);
                self.report_file_transfer_failure(transfer_id, message);
            }
        }
    }
}

fn collect_local_clipboard_files(paths: Vec<PathBuf>) -> Result<Vec<LocalClipboardFile>, String> {
    // Directory descriptors and regular files share one ordered list because
    // subsequent CLIPRDR content requests address that original list by index.
    if paths.is_empty() || paths.len() > RDP_CLIPBOARD_MAX_FILE_COUNT {
        return Err("RDP clipboard root item count is outside the supported range.".to_string());
    }

    let mut seen_roots = HashSet::new();
    let mut seen_wire_paths = HashSet::new();
    let mut total_size = 0_u64;
    let mut files = Vec::new();
    let mut pending = Vec::new();
    for (index, path) in paths.into_iter().enumerate() {
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|_| format!("RDP clipboard item {index} is unavailable."))?;
        if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
            return Err(format!(
                "RDP clipboard item {index} must be a regular file or directory without symbolic links."
            ));
        }
        let canonical_path = std::fs::canonicalize(&path)
            .map_err(|_| format!("RDP clipboard item {index} could not be resolved."))?;
        if !seen_roots.insert(canonical_path.clone()) {
            return Err("RDP clipboard contains duplicate root items.".to_string());
        }
        let name = local_clipboard_file_name(&canonical_path, index)?;
        pending.push((canonical_path, name, None));
    }

    while let Some((path, name, relative_path)) = pending.pop() {
        if files.len() >= RDP_CLIPBOARD_MAX_FILE_COUNT {
            return Err("RDP clipboard item count exceeds the supported limit.".to_string());
        }
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|_| "RDP clipboard item became unavailable.".to_string())?;
        if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
            return Err(
                "RDP clipboard directory tree contains an unsupported symbolic link or item."
                    .to_string(),
            );
        }
        let wire_path = clipboard_wire_path(relative_path.as_deref(), &name);
        if wire_path.encode_utf16().count() > 259
            || !seen_wire_paths.insert(wire_path.to_lowercase())
        {
            return Err(
                "RDP clipboard directory tree contains a duplicate or overlong path.".to_string(),
            );
        }
        let directory = metadata.is_dir();
        let size = if directory { 0 } else { metadata.len() };
        if size > RDP_CLIPBOARD_MAX_FILE_SIZE {
            return Err("RDP clipboard file is too large.".to_string());
        }
        total_size = total_size
            .checked_add(size)
            .filter(|total| *total <= RDP_CLIPBOARD_MAX_TRANSFER_SIZE)
            .ok_or_else(|| "RDP clipboard transfer is too large.".to_string())?;
        files.push(LocalClipboardFile {
            path: path.clone(),
            name: name.clone(),
            relative_path: relative_path.clone(),
            size,
            directory,
        });

        if directory {
            let mut children = std::fs::read_dir(&path)
                .map_err(|_| "RDP clipboard directory could not be read.".to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "RDP clipboard directory entry could not be read.".to_string())?;
            children.sort_by_key(std::fs::DirEntry::file_name);
            for child in children.into_iter().rev() {
                let child_path = child.path();
                let child_name = child
                    .file_name()
                    .to_str()
                    .filter(|name| remote_clipboard_file_name_is_safe(name))
                    .ok_or_else(|| {
                        "RDP clipboard directory contains an unsupported file name.".to_string()
                    })?
                    .to_string();
                pending.push((child_path, child_name, Some(wire_path.clone())));
            }
        }
    }

    Ok(files)
}

fn local_clipboard_file_name(path: &std::path::Path, index: usize) -> Result<String, String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| remote_clipboard_file_name_is_safe(name))
        .map(str::to_string)
        .ok_or_else(|| format!("RDP clipboard item {index} has an unsupported file name."))
}

fn clipboard_wire_path(relative_path: Option<&str>, name: &str) -> String {
    match relative_path {
        Some(relative_path) if !relative_path.is_empty() => format!("{relative_path}\\{name}"),
        _ => name.to_string(),
    }
}

enum RemoteClipboardDownloadAdvance {
    Request(FileContentsRequest),
    Complete(Vec<PathBuf>),
}

enum RemoteClipboardDownloadStart {
    Request(RemoteClipboardDownload, FileContentsRequest),
    Complete(Vec<PathBuf>),
}

fn remote_clipboard_session_directory() -> PathBuf {
    let root = std::env::temp_dir().join("oxideterm-rdp-clipboard");
    cleanup_stale_remote_clipboard_directories(&root);
    let directory = root.join(uuid::Uuid::new_v4().to_string());
    let _ = std::fs::create_dir_all(&directory);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700));
    }
    directory
}

fn cleanup_stale_remote_clipboard_directories(root: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let stale = entry
            .metadata()
            .ok()
            .filter(|metadata| metadata.is_dir())
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= RDP_CLIPBOARD_STAGING_MAX_AGE);
        if stale {
            // Only session UUID directories under OxideTerm's dedicated temp
            // root are eligible for retention cleanup.
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

fn prepare_remote_clipboard_download(
    session_directory: PathBuf,
    transfer_id: String,
    files: &[FileDescriptor],
    clip_data_id: Option<u32>,
) -> Result<RemoteClipboardDownloadStart, String> {
    // Validate the complete remote tree before creating any file so relative
    // paths can never escape or reinterpret an earlier regular-file entry.
    if files.is_empty() || files.len() > RDP_CLIPBOARD_MAX_FILE_COUNT {
        return Err("Remote clipboard file count is outside the supported range.".to_string());
    }
    let mut total_size = 0_u64;
    let mut wire_paths = HashSet::new();
    let mut file_paths = HashSet::new();
    let mut top_level_names = HashSet::new();
    let mut top_level_paths = Vec::new();
    let mut descriptors = Vec::with_capacity(files.len());
    let mut descriptor_components = Vec::with_capacity(files.len());
    let mut file_indices = Vec::new();
    for (index, descriptor) in files.iter().enumerate() {
        let components = remote_clipboard_descriptor_components(descriptor).ok_or_else(|| {
            format!("Remote clipboard item {index} has an unsupported relative path.")
        })?;
        let normalized_path = components.join("\\").to_lowercase();
        if !wire_paths.insert(normalized_path.clone()) {
            return Err("Remote clipboard contains duplicate paths.".to_string());
        }
        let directory = descriptor
            .attributes
            .is_some_and(|attributes| attributes.contains(ClipboardFileAttributes::DIRECTORY));
        if !directory {
            file_paths.insert(normalized_path);
        }
        if let Some(size) = descriptor.file_size {
            if !directory && size > RDP_CLIPBOARD_MAX_FILE_SIZE {
                return Err(format!("Remote clipboard file {index} is too large."));
            }
            if !directory {
                total_size = total_size
                    .checked_add(size)
                    .filter(|total| *total <= RDP_CLIPBOARD_MAX_TRANSFER_SIZE)
                    .ok_or_else(|| "Remote clipboard transfer is too large.".to_string())?;
            }
        }
        if !directory {
            file_indices.push(index);
        }
        if top_level_names.insert(components[0].to_lowercase()) {
            top_level_paths.push(components[0].clone());
        }
        descriptors.push(descriptor.clone());
        descriptor_components.push(components);
    }

    for components in &descriptor_components {
        for ancestor_length in 1..components.len() {
            let ancestor = components[..ancestor_length].join("\\").to_lowercase();
            if file_paths.contains(&ancestor) {
                return Err(
                    "Remote clipboard path places an item below a regular file.".to_string()
                );
            }
        }
    }

    let directory = session_directory.join(&transfer_id);
    std::fs::create_dir_all(&directory)
        .map_err(|_| "Remote clipboard staging directory could not be created.".to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Remote clipboard staging permissions could not be set.".to_string())?;
    }

    for (descriptor, components) in descriptors.iter().zip(&descriptor_components) {
        let path = components
            .iter()
            .fold(directory.clone(), |path, component| path.join(component));
        let is_directory = descriptor
            .attributes
            .is_some_and(|attributes| attributes.contains(ClipboardFileAttributes::DIRECTORY));
        if is_directory {
            std::fs::create_dir_all(&path)
                .map_err(|_| "Remote clipboard directory could not be created.".to_string())?;
        } else if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| {
                "Remote clipboard parent directory could not be created.".to_string()
            })?;
        }
    }
    let completed_paths = top_level_paths
        .into_iter()
        .map(|name| directory.join(name))
        .collect::<Vec<_>>();
    if file_indices.is_empty() {
        return Ok(RemoteClipboardDownloadStart::Complete(completed_paths));
    }

    let stream_id = 1;
    let request = remote_clipboard_size_request(file_indices[0], stream_id, clip_data_id);
    Ok(RemoteClipboardDownloadStart::Request(
        RemoteClipboardDownload {
            transfer_id,
            directory,
            descriptors,
            file_indices,
            clip_data_id,
            file_cursor: 0,
            completed_paths,
            completed_bytes: 0,
            phase: RemoteClipboardDownloadPhase::Size { stream_id },
            next_stream_id: stream_id + 1,
        },
        request,
    ))
}

fn remote_clipboard_descriptor_components(descriptor: &FileDescriptor) -> Option<Vec<String>> {
    let mut components = Vec::new();
    if let Some(relative_path) = descriptor.relative_path.as_deref() {
        if relative_path.is_empty()
            || relative_path.starts_with(['/', '\\'])
            || relative_path.encode_utf16().count() > 259
        {
            return None;
        }
        for component in relative_path.split(['/', '\\']) {
            if !remote_clipboard_file_name_is_safe(component) {
                return None;
            }
            components.push(component.to_string());
        }
    }
    if !remote_clipboard_file_name_is_safe(&descriptor.name) {
        return None;
    }
    components.push(descriptor.name.clone());
    (components.join("\\").encode_utf16().count() <= 259).then_some(components)
}

fn advance_remote_clipboard_download(
    download: &mut RemoteClipboardDownload,
    response: FileContentsResponse<'_>,
) -> Result<RemoteClipboardDownloadAdvance, String> {
    if response.is_error() {
        return Err("The RDP server rejected a clipboard file request.".to_string());
    }
    match download.phase {
        RemoteClipboardDownloadPhase::Size { stream_id } => {
            if response.stream_id() != stream_id {
                return Err("The RDP server returned a mismatched clipboard stream.".to_string());
            }
            let size = response.data_as_size().map_err(|_| {
                "The RDP server returned an invalid clipboard file size.".to_string()
            })?;
            let descriptor_index = download.file_indices[download.file_cursor];
            let descriptor = &download.descriptors[descriptor_index];
            if size > RDP_CLIPBOARD_MAX_FILE_SIZE
                || descriptor
                    .file_size
                    .is_some_and(|expected| expected != size)
            {
                return Err(
                    "The RDP server returned an unexpected clipboard file size.".to_string()
                );
            }
            if download
                .completed_bytes
                .checked_add(size)
                .is_none_or(|total| total > RDP_CLIPBOARD_MAX_TRANSFER_SIZE)
            {
                return Err("Remote clipboard transfer is too large.".to_string());
            }
            let part_path = remote_clipboard_part_path(download);
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&part_path)
                .map_err(|_| "Remote clipboard staging file could not be created.".to_string())?;
            if size == 0 {
                return finish_remote_clipboard_file(download, 0);
            }
            let requested_size =
                u32::try_from(size.min(u64::from(RDP_CLIPBOARD_DOWNLOAD_CHUNK_BYTES)))
                    .unwrap_or(RDP_CLIPBOARD_DOWNLOAD_CHUNK_BYTES);
            let request = remote_clipboard_range_request(
                descriptor_index,
                download.next_stream_id,
                0,
                requested_size,
                download.clip_data_id,
            );
            download.phase = RemoteClipboardDownloadPhase::Data {
                stream_id: download.next_stream_id,
                file_size: size,
                position: 0,
                requested_size,
            };
            download.next_stream_id = download.next_stream_id.saturating_add(1).max(1);
            Ok(RemoteClipboardDownloadAdvance::Request(request))
        }
        RemoteClipboardDownloadPhase::Data {
            stream_id,
            file_size,
            position,
            requested_size,
        } => {
            if response.stream_id() != stream_id
                || response.data().is_empty()
                || response.data().len() > requested_size as usize
            {
                return Err("The RDP server returned an invalid clipboard file chunk.".to_string());
            }
            let next_position = position
                .checked_add(response.data().len() as u64)
                .filter(|position| *position <= file_size)
                .ok_or_else(|| {
                    "The RDP server exceeded the advertised clipboard file size.".to_string()
                })?;
            let part_path = remote_clipboard_part_path(download);
            OpenOptions::new()
                .append(true)
                .open(&part_path)
                .and_then(|mut file| file.write_all(response.data()))
                .map_err(|_| "Remote clipboard file could not be written.".to_string())?;
            if next_position == file_size {
                return finish_remote_clipboard_file(download, file_size);
            }
            let remaining = file_size - next_position;
            let requested_size =
                u32::try_from(remaining.min(u64::from(RDP_CLIPBOARD_DOWNLOAD_CHUNK_BYTES)))
                    .unwrap_or(RDP_CLIPBOARD_DOWNLOAD_CHUNK_BYTES);
            let request = remote_clipboard_range_request(
                download.file_indices[download.file_cursor],
                download.next_stream_id,
                next_position,
                requested_size,
                download.clip_data_id,
            );
            download.phase = RemoteClipboardDownloadPhase::Data {
                stream_id: download.next_stream_id,
                file_size,
                position: next_position,
                requested_size,
            };
            download.next_stream_id = download.next_stream_id.saturating_add(1).max(1);
            Ok(RemoteClipboardDownloadAdvance::Request(request))
        }
    }
}

fn finish_remote_clipboard_file(
    download: &mut RemoteClipboardDownload,
    file_size: u64,
) -> Result<RemoteClipboardDownloadAdvance, String> {
    let part_path = remote_clipboard_part_path(download);
    let final_path = remote_clipboard_final_path(download);
    std::fs::rename(&part_path, &final_path)
        .map_err(|_| "Remote clipboard file could not be finalized.".to_string())?;
    download.completed_bytes = download
        .completed_bytes
        .checked_add(file_size)
        .ok_or_else(|| "Remote clipboard transfer size overflowed.".to_string())?;
    download.file_cursor += 1;
    if download.file_cursor == download.file_indices.len() {
        return Ok(RemoteClipboardDownloadAdvance::Complete(
            download.completed_paths.clone(),
        ));
    }
    let stream_id = download.next_stream_id;
    download.next_stream_id = download.next_stream_id.saturating_add(1).max(1);
    download.phase = RemoteClipboardDownloadPhase::Size { stream_id };
    Ok(RemoteClipboardDownloadAdvance::Request(
        remote_clipboard_size_request(
            download.file_indices[download.file_cursor],
            stream_id,
            download.clip_data_id,
        ),
    ))
}

fn remote_clipboard_size_request(
    file_index: usize,
    stream_id: u32,
    data_id: Option<u32>,
) -> FileContentsRequest {
    FileContentsRequest {
        stream_id,
        index: i32::try_from(file_index).unwrap_or(i32::MAX),
        flags: FileContentsFlags::SIZE,
        position: 0,
        requested_size: 8,
        data_id,
    }
}

fn remote_clipboard_range_request(
    file_index: usize,
    stream_id: u32,
    position: u64,
    requested_size: u32,
    data_id: Option<u32>,
) -> FileContentsRequest {
    FileContentsRequest {
        stream_id,
        index: i32::try_from(file_index).unwrap_or(i32::MAX),
        flags: FileContentsFlags::RANGE,
        position,
        requested_size,
        data_id,
    }
}

fn remote_clipboard_part_path(download: &RemoteClipboardDownload) -> PathBuf {
    let final_path = remote_clipboard_final_path(download);
    // The transfer UUID is not disclosed to the peer, so a remote descriptor
    // cannot collide with this sibling staging name deliberately.
    final_path.with_file_name(format!(
        ".oxideterm-{}-{}.part",
        download.transfer_id, download.file_cursor
    ))
}

fn remote_clipboard_final_path(download: &RemoteClipboardDownload) -> PathBuf {
    let descriptor = &download.descriptors[download.file_indices[download.file_cursor]];
    let mut path = download.directory.clone();
    if let Some(relative_path) = descriptor.relative_path.as_deref() {
        for component in relative_path.split(['/', '\\']) {
            path.push(component);
        }
    }
    path.join(&descriptor.name)
}

fn cleanup_remote_clipboard_download(download: &RemoteClipboardDownload) {
    let _ = std::fs::remove_dir_all(&download.directory);
}

fn remote_clipboard_file_name_is_safe(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 259
        && !name
            .chars()
            .any(|character| character.is_control() || "<>:\"/\\|?*".contains(character))
        && !matches!(name, "." | "..")
        && !name.ends_with(['.', ' '])
        && !ironrdp::cliprdr::is_windows_device_name(name)
}

#[cfg(test)]
mod file_transfer_tests {
    use super::*;

    #[test]
    fn remote_file_download_requests_size_then_bounded_data() {
        let directory = tempfile::tempdir().unwrap();
        let descriptor = FileDescriptor::new("example.txt").with_file_size(5);
        let RemoteClipboardDownloadStart::Request(mut download, size_request) =
            prepare_remote_clipboard_download(
                directory.path().to_path_buf(),
                "transfer".to_string(),
                &[descriptor],
                Some(7),
            )
            .unwrap()
        else {
            panic!("regular file download completed before requesting content");
        };

        assert_eq!(size_request.flags, FileContentsFlags::SIZE);
        let data_request = match advance_remote_clipboard_download(
            &mut download,
            FileContentsResponse::new_size_response(size_request.stream_id, 5),
        )
        .unwrap()
        {
            RemoteClipboardDownloadAdvance::Request(request) => request,
            RemoteClipboardDownloadAdvance::Complete(_) => panic!("size response completed early"),
        };
        assert_eq!(data_request.flags, FileContentsFlags::RANGE);
        assert_eq!(data_request.requested_size, 5);

        let paths = match advance_remote_clipboard_download(
            &mut download,
            FileContentsResponse::new_data_response(data_request.stream_id, b"hello".to_vec()),
        )
        .unwrap()
        {
            RemoteClipboardDownloadAdvance::Complete(paths) => paths,
            RemoteClipboardDownloadAdvance::Request(_) => panic!("file did not complete"),
        };
        assert_eq!(std::fs::read(&paths[0]).unwrap(), b"hello");
    }

    #[test]
    fn clipboard_directories_preserve_relative_paths_in_both_directions() {
        let directory = tempfile::tempdir().unwrap();
        let unsafe_name = FileDescriptor::new("../secret.txt").with_file_size(1);
        assert!(
            prepare_remote_clipboard_download(
                directory.path().to_path_buf(),
                "unsafe".to_string(),
                &[unsafe_name],
                None,
            )
            .is_err()
        );

        let local_root = directory.path().join("folder");
        let local_nested = local_root.join("nested");
        std::fs::create_dir_all(&local_nested).unwrap();
        std::fs::write(local_nested.join("example.txt"), b"hello").unwrap();
        let local_files = collect_local_clipboard_files(vec![local_root]).unwrap();
        assert!(local_files.iter().any(|file| {
            file.name == "example.txt"
                && file.relative_path.as_deref() == Some("folder\\nested")
                && !file.directory
        }));

        let directory_entry = FileDescriptor::new("folder")
            .with_attributes(ClipboardFileAttributes::DIRECTORY)
            .with_file_size(0);
        let RemoteClipboardDownloadStart::Complete(empty_directory_paths) =
            prepare_remote_clipboard_download(
                directory.path().to_path_buf(),
                "empty-directory".to_string(),
                std::slice::from_ref(&directory_entry),
                None,
            )
            .unwrap()
        else {
            panic!("empty directory requested file content");
        };
        assert!(empty_directory_paths[0].is_dir());
        let nested_entry = FileDescriptor::new("nested")
            .with_relative_path("folder")
            .with_attributes(ClipboardFileAttributes::DIRECTORY)
            .with_file_size(0);
        let file_entry = FileDescriptor::new("example.txt")
            .with_relative_path("folder\\nested")
            .with_file_size(5);

        let RemoteClipboardDownloadStart::Request(mut download, size_request) =
            prepare_remote_clipboard_download(
                directory.path().to_path_buf(),
                "directory".to_string(),
                &[directory_entry, nested_entry, file_entry],
                None,
            )
            .unwrap()
        else {
            panic!("directory with a file completed before requesting content");
        };
        assert_eq!(size_request.index, 2);
        let data_request = match advance_remote_clipboard_download(
            &mut download,
            FileContentsResponse::new_size_response(size_request.stream_id, 5),
        )
        .unwrap()
        {
            RemoteClipboardDownloadAdvance::Request(request) => request,
            RemoteClipboardDownloadAdvance::Complete(_) => panic!("size response completed early"),
        };
        let paths = match advance_remote_clipboard_download(
            &mut download,
            FileContentsResponse::new_data_response(data_request.stream_id, b"hello".to_vec()),
        )
        .unwrap()
        {
            RemoteClipboardDownloadAdvance::Complete(paths) => paths,
            RemoteClipboardDownloadAdvance::Request(_) => panic!("directory did not complete"),
        };
        assert_eq!(
            std::fs::read(paths[0].join("nested").join("example.txt")).unwrap(),
            b"hello"
        );
    }
}

pub(super) fn process_clipboard_message(
    active_stage: &mut ActiveStage,
    message: ClipboardMessage,
) -> SessionResult<Vec<ActiveStageOutput>> {
    let Some(svc_messages) = ({
        let Some(cliprdr) = active_stage.get_svc_processor_mut::<CliprdrClient>() else {
            return Ok(Vec::new());
        };
        match message {
            ClipboardMessage::SendInitiateCopy(formats) => Some(
                cliprdr
                    .initiate_copy(&formats)
                    .map_err(|error| session::custom_err!("CLIPRDR initiate copy", error))?,
            ),
            ClipboardMessage::SendFormatData(response) => Some(
                cliprdr
                    .submit_format_data(response)
                    .map_err(|error| session::custom_err!("CLIPRDR format data", error))?,
            ),
            ClipboardMessage::SendInitiatePaste(format) => Some(
                cliprdr
                    .initiate_paste(format)
                    .map_err(|error| session::custom_err!("CLIPRDR initiate paste", error))?,
            ),
            ClipboardMessage::SendFileContentsRequest(request) => Some(
                cliprdr
                    .request_file_contents(request)
                    .map_err(|error| session::custom_err!("CLIPRDR file request", error))?,
            ),
            ClipboardMessage::SendFileContentsResponse(response) => Some(
                cliprdr
                    .submit_file_contents(response)
                    .map_err(|error| session::custom_err!("CLIPRDR file response", error))?,
            ),
            ClipboardMessage::SendInitiateFileCopy(files) => Some(
                cliprdr
                    .initiate_file_copy(files)
                    .map_err(|error| session::custom_err!("CLIPRDR initiate file copy", error))?,
            ),
            ClipboardMessage::Error(_) => None,
        }
    }) else {
        return Ok(Vec::new());
    };

    let frame = active_stage.process_svc_processor_messages(svc_messages)?;
    response_frame_output(frame)
}

pub(super) fn advertise_local_clipboard_text(
    active_stage: &mut ActiveStage,
    text: String,
) -> SessionResult<Vec<ActiveStageOutput>> {
    let Some(cliprdr) = active_stage.get_svc_processor_mut::<CliprdrClient>() else {
        return Ok(Vec::new());
    };
    if let Some(backend) = cliprdr.downcast_backend_mut::<ClientClipboardBackend>() {
        backend.set_local_text(text);
    }

    // If CLIPRDR is not fully ready yet, the backend keeps the text and the
    // initialization callback will advertise it later.
    let Ok(svc_messages) = cliprdr.initiate_copy(&text_clipboard_formats()) else {
        return Ok(Vec::new());
    };
    let frame = active_stage.process_svc_processor_messages(svc_messages)?;
    response_frame_output(frame)
}

pub(super) fn advertise_local_clipboard_data(
    active_stage: &mut ActiveStage,
    data: RemoteDesktopClipboardData,
) -> SessionResult<Vec<ActiveStageOutput>> {
    let Some(cliprdr) = active_stage.get_svc_processor_mut::<CliprdrClient>() else {
        return Ok(Vec::new());
    };
    let formats = image_clipboard_formats(data.format);
    if let Some(backend) = cliprdr.downcast_backend_mut::<ClientClipboardBackend>() {
        backend.set_local_data(data);
    }

    // If CLIPRDR is not fully ready yet, the backend keeps the data and the
    // initialization callback will advertise it later.
    let Ok(svc_messages) = cliprdr.initiate_copy(&formats) else {
        return Ok(Vec::new());
    };
    let frame = active_stage.process_svc_processor_messages(svc_messages)?;
    response_frame_output(frame)
}

pub(super) fn advertise_local_clipboard_files(
    active_stage: &mut ActiveStage,
    transfer_id: String,
    paths: Vec<PathBuf>,
) -> SessionResult<Vec<ActiveStageOutput>> {
    let Some(cliprdr) = active_stage.get_svc_processor_mut::<CliprdrClient>() else {
        return Ok(Vec::new());
    };
    let descriptors = match cliprdr.downcast_backend_mut::<ClientClipboardBackend>() {
        Some(backend) => match backend.set_local_files(transfer_id.clone(), paths) {
            Ok(descriptors) => descriptors,
            Err(message) => {
                backend.report_file_transfer_failure(transfer_id, message);
                return Ok(Vec::new());
            }
        },
        None => return Ok(Vec::new()),
    };
    let svc_messages = match cliprdr.initiate_file_copy(descriptors) {
        Ok(messages) => messages,
        Err(error) => {
            if let Some(backend) = cliprdr.downcast_backend_mut::<ClientClipboardBackend>() {
                backend.report_file_transfer_failure(
                    transfer_id,
                    format!("RDP clipboard file transfer could not start: {error}"),
                );
            }
            return Ok(Vec::new());
        }
    };
    let frame = active_stage.process_svc_processor_messages(svc_messages)?;
    response_frame_output(frame)
}

pub(super) fn cancel_clipboard_transfer(active_stage: &mut ActiveStage, transfer_id: &str) {
    let Some(cliprdr) = active_stage.get_svc_processor_mut::<CliprdrClient>() else {
        return;
    };
    if let Some(backend) = cliprdr.downcast_backend_mut::<ClientClipboardBackend>() {
        backend.cancel_transfer(transfer_id);
    }
}

pub(super) fn drive_clipboard_timeouts(
    active_stage: &mut ActiveStage,
) -> SessionResult<Vec<ActiveStageOutput>> {
    let Some(svc_messages) = ({
        let Some(cliprdr) = active_stage.get_svc_processor_mut::<CliprdrClient>() else {
            return Ok(Vec::new());
        };
        Some(
            cliprdr
                .drive_timeouts()
                .map_err(|error| session::custom_err!("CLIPRDR timeout cleanup", error))?,
        )
    }) else {
        return Ok(Vec::new());
    };
    let frame = active_stage.process_svc_processor_messages(svc_messages)?;
    response_frame_output(frame)
}

pub(super) fn text_clipboard_formats() -> Vec<ClipboardFormat> {
    vec![
        ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT),
        ClipboardFormat::new(ClipboardFormatId::CF_TEXT),
    ]
}

pub(super) fn image_clipboard_formats(
    format: RemoteDesktopClipboardFormat,
) -> Vec<ClipboardFormat> {
    let (id, name) = local_image_clipboard_format(format);
    let mut formats = vec![ClipboardFormat::new(id).with_name(ClipboardFormatName::new(name))];
    if format == RemoteDesktopClipboardFormat::ImagePng {
        // Windows peers commonly request bitmap clipboard data even when a PNG
        // registered format is also available.
        formats.push(ClipboardFormat::new(ClipboardFormatId::CF_DIBV5));
        formats.push(ClipboardFormat::new(ClipboardFormatId::CF_DIB));
    }
    if format == RemoteDesktopClipboardFormat::ImageTiff {
        // TIFF is one of the standard Win32 clipboard formats. Advertise it
        // alongside the registered MIME name so older peers can request it.
        formats.push(ClipboardFormat::new(ClipboardFormatId::CF_TIFF));
    }
    formats
}

pub(super) fn preferred_text_clipboard_format(
    formats: &[ClipboardFormat],
) -> Option<ClipboardFormatId> {
    formats
        .iter()
        .find(|format| format.id == ClipboardFormatId::CF_UNICODETEXT)
        .or_else(|| {
            formats
                .iter()
                .find(|format| format.id == ClipboardFormatId::CF_TEXT)
        })
        .map(|format| format.id)
}

pub(super) fn preferred_image_clipboard_format(
    formats: &[ClipboardFormat],
) -> Option<RdpClipboardDataFormat> {
    formats
        .iter()
        .find_map(rdp_clipboard_data_format_from_named_format)
        .or_else(|| {
            formats
                .iter()
                .find(|format| format.id == ClipboardFormatId::CF_DIBV5)
                .map(|format| RdpClipboardDataFormat {
                    id: format.id,
                    format: RemoteDesktopClipboardFormat::ImagePng,
                    encoding: RdpClipboardDataEncoding::DibV5,
                })
        })
        .or_else(|| {
            formats
                .iter()
                .find(|format| format.id == ClipboardFormatId::CF_DIB)
                .map(|format| RdpClipboardDataFormat {
                    id: format.id,
                    format: RemoteDesktopClipboardFormat::ImagePng,
                    encoding: RdpClipboardDataEncoding::Dib,
                })
        })
        .or_else(|| {
            formats
                .iter()
                .find(|format| format.id == ClipboardFormatId::CF_TIFF)
                .map(|format| RdpClipboardDataFormat {
                    id: format.id,
                    format: RemoteDesktopClipboardFormat::ImageTiff,
                    encoding: RdpClipboardDataEncoding::Encoded,
                })
        })
}

pub(super) fn decode_remote_clipboard_data(
    format: RdpClipboardDataFormat,
    bytes: Vec<u8>,
) -> Option<RemoteDesktopClipboardData> {
    if bytes.is_empty() {
        return None;
    }
    let bytes = match format.encoding {
        RdpClipboardDataEncoding::Encoded => bytes,
        RdpClipboardDataEncoding::Dib => dib_to_png(&bytes).ok()?,
        RdpClipboardDataEncoding::DibV5 => dibv5_to_png(&bytes).ok()?,
    };
    Some(RemoteDesktopClipboardData::new(format.format, bytes))
}

pub(super) fn local_image_clipboard_format(
    format: RemoteDesktopClipboardFormat,
) -> (ClipboardFormatId, &'static str) {
    match format {
        RemoteDesktopClipboardFormat::ImagePng => (RDP_CLIPBOARD_FORMAT_IMAGE_PNG, "PNG"),
        RemoteDesktopClipboardFormat::ImageJpeg => (RDP_CLIPBOARD_FORMAT_IMAGE_JPEG, "JFIF"),
        RemoteDesktopClipboardFormat::ImageWebp => (RDP_CLIPBOARD_FORMAT_IMAGE_WEBP, "image/webp"),
        RemoteDesktopClipboardFormat::ImageGif => (RDP_CLIPBOARD_FORMAT_IMAGE_GIF, "GIF"),
        RemoteDesktopClipboardFormat::ImageSvg => (RDP_CLIPBOARD_FORMAT_IMAGE_SVG, "image/svg+xml"),
        RemoteDesktopClipboardFormat::ImageBmp => (RDP_CLIPBOARD_FORMAT_IMAGE_BMP, "image/bmp"),
        RemoteDesktopClipboardFormat::ImageTiff => (RDP_CLIPBOARD_FORMAT_IMAGE_TIFF, "image/tiff"),
    }
}

pub(super) fn local_image_clipboard_format_ids(
    format: RemoteDesktopClipboardFormat,
) -> Vec<ClipboardFormatId> {
    let (id, _) = local_image_clipboard_format(format);
    let mut ids = vec![id];
    if format == RemoteDesktopClipboardFormat::ImagePng {
        ids.push(ClipboardFormatId::CF_DIBV5);
        ids.push(ClipboardFormatId::CF_DIB);
    }
    if format == RemoteDesktopClipboardFormat::ImageTiff {
        ids.push(ClipboardFormatId::CF_TIFF);
    }
    ids
}

pub(super) fn encode_local_clipboard_data(
    data: &RemoteDesktopClipboardData,
    format: ClipboardFormatId,
) -> Option<Vec<u8>> {
    if !local_image_clipboard_format_ids(data.format).contains(&format) {
        return None;
    }
    match (data.format, format) {
        (RemoteDesktopClipboardFormat::ImagePng, ClipboardFormatId::CF_DIB) => {
            png_to_cf_dib(&data.bytes).ok()
        }
        (RemoteDesktopClipboardFormat::ImagePng, ClipboardFormatId::CF_DIBV5) => {
            png_to_cf_dibv5(&data.bytes).ok()
        }
        _ => Some(data.bytes.clone()),
    }
}

pub(super) fn rdp_clipboard_data_format_from_named_format(
    format: &ClipboardFormat,
) -> Option<RdpClipboardDataFormat> {
    let name = format.name()?.value();
    let clipboard_format = remote_desktop_clipboard_format_from_rdp_name(name)?;
    Some(RdpClipboardDataFormat {
        id: format.id,
        format: clipboard_format,
        encoding: RdpClipboardDataEncoding::Encoded,
    })
}

pub(super) fn remote_desktop_clipboard_format_from_rdp_name(
    name: &str,
) -> Option<RemoteDesktopClipboardFormat> {
    if name.eq_ignore_ascii_case("PNG") || name.eq_ignore_ascii_case("image/png") {
        return Some(RemoteDesktopClipboardFormat::ImagePng);
    }
    if name.eq_ignore_ascii_case("JFIF")
        || name.eq_ignore_ascii_case("JPEG")
        || name.eq_ignore_ascii_case("JPG")
        || name.eq_ignore_ascii_case("image/jpeg")
        || name.eq_ignore_ascii_case("image/jpg")
    {
        return Some(RemoteDesktopClipboardFormat::ImageJpeg);
    }
    if name.eq_ignore_ascii_case("image/webp") {
        return Some(RemoteDesktopClipboardFormat::ImageWebp);
    }
    if name.eq_ignore_ascii_case("GIF") || name.eq_ignore_ascii_case("image/gif") {
        return Some(RemoteDesktopClipboardFormat::ImageGif);
    }
    if name.eq_ignore_ascii_case("image/svg+xml") {
        return Some(RemoteDesktopClipboardFormat::ImageSvg);
    }
    if name.eq_ignore_ascii_case("image/bmp") {
        return Some(RemoteDesktopClipboardFormat::ImageBmp);
    }
    if name.eq_ignore_ascii_case("image/tiff") || name.eq_ignore_ascii_case("image/tif") {
        return Some(RemoteDesktopClipboardFormat::ImageTiff);
    }
    None
}

pub(super) fn clamp_u32_to_u16(value: u32) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}
