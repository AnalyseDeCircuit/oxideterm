// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::Read,
    path::PathBuf,
    sync::mpsc::{self, SyncSender},
    thread::{self, JoinHandle},
};

use oxide_spice_helper_protocol::{HelperEvent, HelperFileTransferState, HelperRequest};

const MAX_ACTIVE_TRANSFERS: usize = 8;
const FILE_TRANSFER_CHUNK_BYTES: usize = 64 * 1024;

pub enum SpiceFileUploadAction {
    Request(HelperRequest),
    Failed { group_id: String, message: String },
}

#[derive(Default)]
pub struct SpiceFileUploadRuntime {
    next_transfer_id: u64,
    pending_groups: VecDeque<PendingUploadGroup>,
    active: HashMap<u64, ActiveUpload>,
    reader_result_tx: Option<mpsc::Sender<FileReaderResult>>,
    reader_result_rx: Option<mpsc::Receiver<FileReaderResult>>,
}

struct PendingUploadGroup {
    group_id: String,
    paths: VecDeque<PathBuf>,
}

struct ActiveUpload {
    group_id: String,
    file_name: String,
    size: Option<u64>,
    submitted_bytes: u64,
    read_pending: bool,
    finish_submitted: bool,
    reader: FileReaderOwner,
}

enum FileReaderCommand {
    Read { bytes: usize },
    Stop,
}

enum FileReaderResult {
    Prepared { transfer_id: u64, size: u64 },
    Chunk { transfer_id: u64, bytes: Vec<u8> },
    Failed { transfer_id: u64 },
}

struct FileReaderOwner {
    command_tx: Option<SyncSender<FileReaderCommand>>,
    thread: Option<JoinHandle<()>>,
}

impl SpiceFileUploadRuntime {
    pub fn start_group(
        &mut self,
        group_id: String,
        paths: Vec<PathBuf>,
    ) -> Vec<SpiceFileUploadAction> {
        if paths.is_empty() {
            return vec![upload_failure(group_id)];
        }
        self.pending_groups.push_back(PendingUploadGroup {
            group_id,
            paths: paths.into(),
        });
        self.fill_available_slots();
        self.poll()
    }

    pub fn cancel_group(&mut self, group_id: &str) -> Vec<SpiceFileUploadAction> {
        self.pending_groups
            .retain(|group| group.group_id != group_id);
        let transfer_ids = self
            .active
            .iter()
            .filter_map(|(transfer_id, upload)| {
                (upload.group_id == group_id).then_some(*transfer_id)
            })
            .collect::<Vec<_>>();
        let mut actions = Vec::with_capacity(transfer_ids.len());
        for transfer_id in transfer_ids {
            self.active.remove(&transfer_id);
            actions.push(SpiceFileUploadAction::Request(
                HelperRequest::FileTransferCancel { transfer_id },
            ));
        }
        self.fill_available_slots();
        actions.extend(self.poll());
        actions
    }

    pub fn handle_event(&mut self, event: &HelperEvent) -> Vec<SpiceFileUploadAction> {
        let HelperEvent::FileTransferState {
            transfer_id,
            state,
            accepted_bytes,
            ..
        } = event
        else {
            return self.poll();
        };
        let mut actions = self.poll();
        let terminal = matches!(
            state,
            HelperFileTransferState::Completed
                | HelperFileTransferState::Cancelled
                | HelperFileTransferState::Failed
                | HelperFileTransferState::AgentDisconnected
        );
        if terminal {
            if let Some(upload) = self.active.remove(transfer_id)
                && matches!(
                    state,
                    HelperFileTransferState::Failed | HelperFileTransferState::AgentDisconnected
                )
            {
                actions.push(upload_failure(upload.group_id));
            }
            self.fill_available_slots();
            actions.extend(self.poll());
            return actions;
        }

        let Some(upload) = self.active.get_mut(transfer_id) else {
            return actions;
        };
        if *state != HelperFileTransferState::Sending {
            return actions;
        }
        let Some(size) = upload.size else {
            return actions;
        };
        if size == 0 && !upload.finish_submitted {
            upload.finish_submitted = true;
            actions.push(SpiceFileUploadAction::Request(
                HelperRequest::FileTransferFinish {
                    transfer_id: *transfer_id,
                },
            ));
        } else if *accepted_bytes == upload.submitted_bytes
            && upload.submitted_bytes < size
            && !upload.read_pending
        {
            let remaining = size.saturating_sub(upload.submitted_bytes);
            let bytes = usize::try_from(remaining)
                .unwrap_or(usize::MAX)
                .min(FILE_TRANSFER_CHUNK_BYTES);
            if upload.reader.read(bytes) {
                upload.read_pending = true;
            } else {
                actions.push(upload_failure(upload.group_id.clone()));
            }
        }
        actions
    }

    pub fn poll(&mut self) -> Vec<SpiceFileUploadAction> {
        let mut actions = Vec::new();
        let results = self
            .reader_result_rx
            .as_ref()
            .map(|receiver| receiver.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for result in results {
            match result {
                FileReaderResult::Prepared { transfer_id, size } => {
                    if let Some(upload) = self.active.get_mut(&transfer_id) {
                        upload.size = Some(size);
                        actions.push(SpiceFileUploadAction::Request(
                            HelperRequest::FileTransferStart {
                                transfer_id,
                                file_name: upload.file_name.clone(),
                                size,
                            },
                        ));
                    }
                }
                FileReaderResult::Chunk { transfer_id, bytes } => {
                    let Some(upload) = self.active.get_mut(&transfer_id) else {
                        continue;
                    };
                    upload.read_pending = false;
                    let Some(size) = upload.size else {
                        continue;
                    };
                    if bytes.is_empty()
                        || upload.submitted_bytes.saturating_add(bytes.len() as u64) > size
                    {
                        let group_id = upload.group_id.clone();
                        self.active.remove(&transfer_id);
                        actions.push(SpiceFileUploadAction::Request(
                            HelperRequest::FileTransferCancel { transfer_id },
                        ));
                        actions.push(upload_failure(group_id));
                        continue;
                    }
                    upload.submitted_bytes =
                        upload.submitted_bytes.saturating_add(bytes.len() as u64);
                    actions.push(SpiceFileUploadAction::Request(
                        HelperRequest::FileTransferData {
                            transfer_id,
                            data: bytes,
                        },
                    ));
                }
                FileReaderResult::Failed { transfer_id } => {
                    if let Some(upload) = self.active.remove(&transfer_id) {
                        if upload.size.is_some() {
                            actions.push(SpiceFileUploadAction::Request(
                                HelperRequest::FileTransferCancel { transfer_id },
                            ));
                        }
                        actions.push(upload_failure(upload.group_id));
                    }
                }
            }
        }
        self.fill_available_slots();
        actions
    }

    fn fill_available_slots(&mut self) {
        while self.active.len() < MAX_ACTIVE_TRANSFERS {
            let Some(group) = self.pending_groups.front_mut() else {
                break;
            };
            let Some(path) = group.paths.pop_front() else {
                self.pending_groups.pop_front();
                continue;
            };
            let group_id = group.group_id.clone();
            if group.paths.is_empty() {
                self.pending_groups.pop_front();
            }
            let file_name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "file".to_string());
            let transfer_id = self.allocate_transfer_id();
            let result_tx = self.reader_result_sender();
            match FileReaderOwner::spawn(transfer_id, path, result_tx) {
                Ok(reader) => {
                    self.active.insert(
                        transfer_id,
                        ActiveUpload {
                            group_id,
                            file_name,
                            size: None,
                            submitted_bytes: 0,
                            read_pending: false,
                            finish_submitted: false,
                            reader,
                        },
                    );
                }
                Err(_) => continue,
            }
        }
    }

    fn reader_result_sender(&mut self) -> mpsc::Sender<FileReaderResult> {
        if let Some(sender) = self.reader_result_tx.as_ref() {
            return sender.clone();
        }
        let (sender, receiver) = mpsc::channel();
        self.reader_result_tx = Some(sender.clone());
        self.reader_result_rx = Some(receiver);
        sender
    }

    fn allocate_transfer_id(&mut self) -> u64 {
        loop {
            self.next_transfer_id = self.next_transfer_id.wrapping_add(1).max(1);
            if !self.active.contains_key(&self.next_transfer_id) {
                return self.next_transfer_id;
            }
        }
    }
}

impl FileReaderOwner {
    fn spawn(
        transfer_id: u64,
        path: PathBuf,
        result_tx: mpsc::Sender<FileReaderResult>,
    ) -> std::io::Result<Self> {
        let (command_tx, command_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name(format!("spice-file-reader-{transfer_id}"))
            .spawn(move || run_file_reader(transfer_id, path, command_rx, result_tx))?;
        Ok(Self {
            command_tx: Some(command_tx),
            thread: Some(thread),
        })
    }

    fn read(&self, bytes: usize) -> bool {
        self.command_tx
            .as_ref()
            .is_some_and(|sender| sender.try_send(FileReaderCommand::Read { bytes }).is_ok())
    }

    fn stop(&mut self) {
        if let Some(sender) = self.command_tx.take() {
            let _ = sender.try_send(FileReaderCommand::Stop);
        }
        let Some(thread) = self.thread.take() else {
            return;
        };
        if thread.is_finished() {
            let _ = thread.join();
            return;
        }
        // Slow network filesystems must not block remote desktop teardown.
        let _ = thread::Builder::new()
            .name("spice-file-reader-reaper".to_string())
            .spawn(move || {
                let _ = thread.join();
            });
    }
}

impl Drop for FileReaderOwner {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_file_reader(
    transfer_id: u64,
    path: PathBuf,
    command_rx: mpsc::Receiver<FileReaderCommand>,
    result_tx: mpsc::Sender<FileReaderResult>,
) {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => {
            let _ = result_tx.send(FileReaderResult::Failed { transfer_id });
            return;
        }
    };
    let size = match file.metadata() {
        Ok(metadata) if metadata.is_file() => metadata.len(),
        _ => {
            let _ = result_tx.send(FileReaderResult::Failed { transfer_id });
            return;
        }
    };
    if result_tx
        .send(FileReaderResult::Prepared { transfer_id, size })
        .is_err()
    {
        return;
    }
    while let Ok(command) = command_rx.recv() {
        match command {
            FileReaderCommand::Read { bytes } => {
                let mut chunk = vec![0; bytes];
                match file.read(&mut chunk) {
                    Ok(read) => {
                        chunk.truncate(read);
                        if result_tx
                            .send(FileReaderResult::Chunk {
                                transfer_id,
                                bytes: chunk,
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(_) => {
                        let _ = result_tx.send(FileReaderResult::Failed { transfer_id });
                        return;
                    }
                }
            }
            FileReaderCommand::Stop => return,
        }
    }
}

fn upload_failure(group_id: String) -> SpiceFileUploadAction {
    SpiceFileUploadAction::Failed {
        group_id,
        message: "The selected file could not be transferred through SPICE.".to_string(),
    }
}
