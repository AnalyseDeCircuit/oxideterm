use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex, mpsc::Sender},
};

use oxideterm_terminal::{TrzszTransferDirection, TrzszTransferPolicy, TrzszTransferSelection};
use oxideterm_trzsz::{
    TRZSZ_API_VERSION, TextProgressBar, TrzszDownloadOpenDto, TrzszError, TrzszFileReader,
    TrzszFileWriter, TrzszSaveParam, TrzszState, TrzszTransfer, TrzszUploadEntryDto, download,
    upload,
};
use serde_json::Value;

#[derive(Debug, Clone)]
pub(crate) struct TrzszPromptRequest {
    pub(crate) direction: TrzszTransferDirection,
    pub(crate) selection: TrzszTransferSelection,
    pub(crate) remote_is_windows: bool,
}

pub(crate) enum TrzszPromptSelection {
    Upload(Vec<String>),
    DownloadRoot(String),
    Cancelled,
}

pub(crate) struct TrzszWorkerJob {
    pub(crate) transfer: TrzszTransfer,
    pub(crate) request: TrzszPromptRequest,
    pub(crate) selection: TrzszPromptSelection,
    pub(crate) owner_id: String,
    pub(crate) state: Arc<TrzszState>,
    pub(crate) policy: TrzszTransferPolicy,
    pub(crate) event_tx: Sender<TrzszWorkerEvent>,
    pub(crate) terminal_columns: usize,
}

pub(crate) enum TrzszWorkerEvent {
    TerminalOutput(Vec<u8>),
    Completed,
    Cancelled,
    PartialCleanup,
    Failed {
        code: String,
        detail: Option<String>,
        message: String,
    },
}

include!("trzsz_worker_upload.rs");

pub(crate) fn run_trzsz_worker_job(mut job: TrzszWorkerJob) -> Result<(), TrzszError> {
    // This is the native equivalent of Tauri's TrzszFilter callbacks: the
    // worker owns local file handles and the blocking protocol loop, while SSH
    // channel IO stays in the terminal session.
    let result = match job.request.direction {
        TrzszTransferDirection::Upload => run_upload(&mut job),
        TrzszTransferDirection::Download => run_download(&mut job),
    };

    if let Err(error) = &result
        && !is_cancelled_transfer(error)
    {
        let _ = job.transfer.client_error(error);
    }
    emit_completion_event(&job, &result);
    let cleanup = job.state.cleanup_owner(&job.owner_id);
    if cleanup.cleanup_errors > 0 {
        let _ = job.event_tx.send(TrzszWorkerEvent::PartialCleanup);
    }
    result
}

fn run_upload(job: &mut TrzszWorkerJob) -> Result<(), TrzszError> {
    let paths = match &job.selection {
        TrzszPromptSelection::Upload(paths) if !paths.is_empty() => paths.clone(),
        _ => {
            job.transfer
                .send_action(false, job.request.remote_is_windows)?;
            return Err(TrzszError::InvalidState("Stopped".to_string()));
        }
    };

    let directory = job.request.selection == TrzszTransferSelection::Directory;
    if directory && !job.policy.allow_directory {
        return Err(TrzszError::DirectoryNotAllowed(
            "terminal settings".to_string(),
        ));
    }

    let readers = build_upload_readers(job.state.clone(), &job.owner_id, paths, &job.policy)?;
    if readers.is_empty() {
        job.transfer
            .send_action(false, job.request.remote_is_windows)?;
        return Err(TrzszError::InvalidState("Stopped".to_string()));
    }

    job.transfer
        .send_action(true, job.request.remote_is_windows)?;
    let config = job.transfer.recv_config()?;
    if config
        .get("overwrite")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        check_duplicate_names(&readers)?;
    }

    let mut progress = progress_from_config(job, &config);
    let send_result = job.transfer.send_files(readers, progress.as_mut());
    if let Some(progress) = progress.as_mut() {
        progress.show_cursor();
    }
    let remote_names = send_result?;
    job.transfer
        .client_exit(&format_saved_files(&remote_names, ""))
}

fn run_download(job: &mut TrzszWorkerJob) -> Result<(), TrzszError> {
    let root_path = match &job.selection {
        TrzszPromptSelection::DownloadRoot(root_path) => root_path.clone(),
        _ => {
            job.transfer
                .send_action(false, job.request.remote_is_windows)?;
            return Err(TrzszError::InvalidState("Stopped".to_string()));
        }
    };

    let prepared =
        download::prepare_download_root(&job.state, &job.owner_id, TRZSZ_API_VERSION, root_path)?;
    job.transfer
        .send_action(true, job.request.remote_is_windows)?;
    let config = job.transfer.recv_config()?;
    let directory = config
        .get("directory")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if directory && !job.policy.allow_directory {
        return Err(TrzszError::DirectoryNotAllowed(
            "terminal settings".to_string(),
        ));
    }

    let display_name = base_name(&prepared.root_path);
    let mut save_root = NativeSaveRoot {
        root_path: prepared.root_path,
        display_name,
        maps: HashMap::new(),
    };
    let mut constraints = DownloadConstraintTracker::new(job.policy.clone());
    let state = job.state.clone();
    let owner_id = job.owner_id.clone();
    let mut progress = progress_from_config(job, &config);
    let recv_result = job.transfer.recv_files(
        &TrzszSaveParam {
            root_path: save_root.root_path.clone(),
            display_name: save_root.display_name.clone(),
        },
        |save_param, file_name, directory, overwrite| {
            open_save_file(
                state.clone(),
                owner_id.clone(),
                &mut save_root,
                save_param,
                file_name,
                directory,
                overwrite,
                &mut constraints,
            )
        },
        progress.as_mut(),
    );
    if let Some(progress) = progress.as_mut() {
        progress.show_cursor();
    }
    let local_names = recv_result?;
    job.transfer
        .client_exit(&format_saved_files(&local_names, &save_root.root_path))
}

fn progress_from_config(job: &TrzszWorkerJob, config: &Value) -> Option<TextProgressBar> {
    if config
        .get("quiet")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }

    let tmux_pane_columns = config
        .get("tmux_pane_width")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok());
    let event_tx = job.event_tx.clone();
    let writer = Arc::new(move |output: String| {
        let _ = event_tx.send(TrzszWorkerEvent::TerminalOutput(output.into_bytes()));
    });
    let mut progress =
        TextProgressBar::new_with_writer(job.terminal_columns.max(1), tmux_pane_columns, writer);
    progress.hide_cursor();
    Some(progress)
}

fn emit_completion_event(job: &TrzszWorkerJob, result: &Result<(), TrzszError>) {
    let event = match result {
        Ok(()) => TrzszWorkerEvent::Completed,
        Err(error) if is_cancelled_transfer(error) => TrzszWorkerEvent::Cancelled,
        Err(error) => TrzszWorkerEvent::Failed {
            code: error.code().as_str().to_string(),
            detail: error.detail(),
            message: error.to_string(),
        },
    };
    let _ = job.event_tx.send(event);
}

struct NativeSaveRoot {
    root_path: String,
    display_name: String,
    maps: HashMap<u64, String>,
}

struct DownloadConstraintTracker {
    policy: TrzszTransferPolicy,
    file_count: usize,
    total_bytes: Arc<Mutex<u64>>,
}

impl DownloadConstraintTracker {
    fn new(policy: TrzszTransferPolicy) -> Self {
        Self {
            policy,
            file_count: 0,
            total_bytes: Arc::new(Mutex::new(0)),
        }
    }

    fn ensure_directory_allowed(&self) -> Result<(), TrzszError> {
        if self.policy.allow_directory {
            Ok(())
        } else {
            Err(TrzszError::DirectoryNotAllowed(
                "terminal settings".to_string(),
            ))
        }
    }

    fn assert_can_add_file(&self) -> Result<(), TrzszError> {
        if self.file_count + 1 > self.policy.max_file_count {
            Err(TrzszError::MaxFileCountExceeded {
                selected: self.file_count + 1,
                max: self.policy.max_file_count,
            })
        } else {
            Ok(())
        }
    }

    fn commit_file(&mut self) {
        self.file_count = self.file_count.saturating_add(1);
    }

    fn byte_counter(&self) -> Arc<Mutex<u64>> {
        self.total_bytes.clone()
    }
}

fn open_save_file(
    state: Arc<TrzszState>,
    owner_id: String,
    save_root: &mut NativeSaveRoot,
    save_param: &TrzszSaveParam,
    file_name: &str,
    directory: bool,
    overwrite: bool,
    constraints: &mut DownloadConstraintTracker,
) -> Result<Box<dyn TrzszFileWriter>, TrzszError> {
    if !directory {
        return open_flat_save_file(
            state,
            owner_id,
            save_param,
            file_name,
            overwrite,
            constraints,
        );
    }

    let entry = parse_directory_entry(file_name)?;
    open_directory_save_entry(state, owner_id, save_root, entry, overwrite, constraints)
}

fn open_flat_save_file(
    state: Arc<TrzszState>,
    owner_id: String,
    save_param: &TrzszSaveParam,
    file_name: &str,
    overwrite: bool,
    constraints: &mut DownloadConstraintTracker,
) -> Result<Box<dyn TrzszFileWriter>, TrzszError> {
    let mut last_error = None;
    for attempt in 0..1000 {
        let candidate = if overwrite {
            file_name.to_string()
        } else {
            next_collision_name(file_name, attempt)
        };
        match try_open_download_file(
            state.clone(),
            owner_id.clone(),
            save_param.root_path.clone(),
            candidate.clone(),
            file_name.to_string(),
            candidate.clone(),
            Vec::new(),
            overwrite,
            constraints,
        ) {
            Ok(writer) => return Ok(Box::new(writer)),
            Err(error) if !overwrite && is_retryable_collision(&error) => {
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| TrzszError::InvalidPath(file_name.to_string())))
}

fn open_directory_save_entry(
    state: Arc<TrzszState>,
    owner_id: String,
    save_root: &mut NativeSaveRoot,
    entry: TrzszDirectoryEntry,
    overwrite: bool,
    constraints: &mut DownloadConstraintTracker,
) -> Result<Box<dyn TrzszFileWriter>, TrzszError> {
    let existing_local_name = if overwrite {
        Some(entry.path_name[0].clone())
    } else {
        save_root.maps.get(&entry.path_id).cloned()
    };
    let rest_path = entry.path_name[1..].to_vec();

    if let Some(local_root) = existing_local_name {
        return try_open_with_root(
            state,
            owner_id,
            save_root,
            &entry,
            rest_path,
            local_root,
            false,
            overwrite,
            constraints,
        );
    }

    let mut last_error = None;
    for attempt in 0..1000 {
        let local_root = if overwrite {
            entry.path_name[0].clone()
        } else {
            next_collision_name(&entry.path_name[0], attempt)
        };
        match try_open_with_root(
            state.clone(),
            owner_id.clone(),
            save_root,
            &entry,
            rest_path.clone(),
            local_root.clone(),
            true,
            overwrite,
            constraints,
        ) {
            Ok(writer) => {
                save_root.maps.insert(entry.path_id, local_root);
                return Ok(writer);
            }
            Err(error) if !overwrite && is_retryable_collision(&error) => last_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| TrzszError::InvalidPath(entry.path_name.join("/"))))
}

#[expect(clippy::too_many_arguments)]
fn try_open_with_root(
    state: Arc<TrzszState>,
    owner_id: String,
    save_root: &NativeSaveRoot,
    entry: &TrzszDirectoryEntry,
    rest_path: Vec<String>,
    local_root: String,
    claim_top_level: bool,
    overwrite: bool,
    constraints: &mut DownloadConstraintTracker,
) -> Result<Box<dyn TrzszFileWriter>, TrzszError> {
    let mut cleanup_directories = Vec::new();
    let result = (|| {
        if entry.is_dir || !rest_path.is_empty() {
            constraints.ensure_directory_allowed()?;
        }

        if claim_top_level && (entry.is_dir || !rest_path.is_empty()) {
            ensure_download_directory(
                &state,
                &owner_id,
                &save_root.root_path,
                &local_root,
                &mut cleanup_directories,
                !overwrite,
            )?;
        }

        let relative_path = join_path(std::iter::once(local_root.clone()).chain(rest_path.clone()));
        if entry.is_dir {
            for index in 0..rest_path.len() {
                let path = join_path(
                    std::iter::once(local_root.clone()).chain(rest_path[..=index].iter().cloned()),
                );
                ensure_download_directory(
                    &state,
                    &owner_id,
                    &save_root.root_path,
                    &path,
                    &mut cleanup_directories,
                    false,
                )?;
            }
            return Ok(Box::new(NativeDirectoryWriter {
                state: state.clone(),
                owner_id: owner_id.clone(),
                root_path: save_root.root_path.clone(),
                cleanup_directories: cleanup_directories.clone(),
                file_name: entry
                    .path_name
                    .last()
                    .cloned()
                    .unwrap_or_else(|| local_root.clone()),
                local_name: local_root,
            }) as Box<dyn TrzszFileWriter>);
        }

        constraints.assert_can_add_file()?;
        for index in 0..rest_path.len().saturating_sub(1) {
            let path = join_path(
                std::iter::once(local_root.clone()).chain(rest_path[..=index].iter().cloned()),
            );
            ensure_download_directory(
                &state,
                &owner_id,
                &save_root.root_path,
                &path,
                &mut cleanup_directories,
                false,
            )?;
        }

        let writer = try_open_download_file(
            state.clone(),
            owner_id.clone(),
            save_root.root_path.clone(),
            relative_path,
            entry
                .path_name
                .last()
                .cloned()
                .unwrap_or_else(|| local_root.clone()),
            local_root,
            cleanup_directories.clone(),
            overwrite,
            constraints,
        )?;
        Ok(Box::new(writer) as Box<dyn TrzszFileWriter>)
    })();

    if result.is_err() {
        for directory_path in cleanup_directories.iter().rev() {
            let _ = download::remove_download_directory(
                &state,
                &owner_id,
                TRZSZ_API_VERSION,
                save_root.root_path.clone(),
                directory_path.clone(),
            );
        }
    }
    result
}

#[expect(clippy::too_many_arguments)]
fn try_open_download_file(
    state: Arc<TrzszState>,
    owner_id: String,
    root_path: String,
    relative_path: String,
    file_name: String,
    local_name: String,
    cleanup_directories: Vec<String>,
    overwrite: bool,
    constraints: &mut DownloadConstraintTracker,
) -> Result<NativeDownloadFileWriter, TrzszError> {
    let dto = download::open_save_file(
        &state,
        &owner_id,
        TRZSZ_API_VERSION,
        root_path.clone(),
        relative_path.clone(),
        false,
        overwrite,
    )?;
    constraints.commit_file();
    Ok(NativeDownloadFileWriter::new(
        state,
        owner_id,
        dto,
        root_path,
        relative_path,
        file_name,
        local_name,
        cleanup_directories,
        constraints,
    ))
}

fn ensure_download_directory(
    state: &TrzszState,
    owner_id: &str,
    root_path: &str,
    directory_path: &str,
    cleanup_directories: &mut Vec<String>,
    must_create: bool,
) -> Result<(), TrzszError> {
    let dto = download::create_download_directory(
        state,
        owner_id,
        TRZSZ_API_VERSION,
        root_path.to_string(),
        directory_path.to_string(),
        must_create,
    )?;
    if dto.created {
        cleanup_directories.push(directory_path.to_string());
    }
    Ok(())
}

struct NativeDirectoryWriter {
    state: Arc<TrzszState>,
    owner_id: String,
    root_path: String,
    cleanup_directories: Vec<String>,
    file_name: String,
    local_name: String,
}

impl TrzszFileWriter for NativeDirectoryWriter {
    fn close_file(&mut self) {}

    fn file_name(&self) -> &str {
        &self.file_name
    }

    fn local_name(&self) -> &str {
        &self.local_name
    }

    fn is_dir(&self) -> bool {
        true
    }

    fn write_file(&mut self, _data: &[u8]) -> Result<(), TrzszError> {
        Err(TrzszError::InvalidState(format!(
            "Cannot write data into directory: {}",
            self.file_name
        )))
    }

    fn delete_file(&mut self) -> Result<String, TrzszError> {
        self.abort_file()?;
        Ok(String::new())
    }

    fn commit_file(&mut self) -> Result<(), TrzszError> {
        for directory_path in &self.cleanup_directories {
            download::commit_download_directory(
                &self.state,
                &self.owner_id,
                TRZSZ_API_VERSION,
                self.root_path.clone(),
                directory_path.clone(),
            )?;
        }
        Ok(())
    }

    fn abort_file(&mut self) -> Result<(), TrzszError> {
        for directory_path in self.cleanup_directories.iter().rev() {
            download::remove_download_directory(
                &self.state,
                &self.owner_id,
                TRZSZ_API_VERSION,
                self.root_path.clone(),
                directory_path.clone(),
            )?;
        }
        Ok(())
    }
}

struct NativeDownloadFileWriter {
    state: Arc<TrzszState>,
    owner_id: String,
    writer_id: String,
    root_path: String,
    relative_path: String,
    file_name: String,
    local_name: String,
    cleanup_directories: Vec<String>,
    finished: bool,
    aborted: bool,
    finish_started: bool,
    total_limit: u64,
    total_bytes: Arc<Mutex<u64>>,
}

impl NativeDownloadFileWriter {
    #[expect(clippy::too_many_arguments)]
    fn new(
        state: Arc<TrzszState>,
        owner_id: String,
        dto: TrzszDownloadOpenDto,
        root_path: String,
        relative_path: String,
        file_name: String,
        local_name: String,
        cleanup_directories: Vec<String>,
        constraints: &DownloadConstraintTracker,
    ) -> Self {
        Self {
            state,
            owner_id,
            writer_id: dto.writer_id,
            root_path,
            relative_path,
            file_name,
            local_name,
            cleanup_directories,
            finished: false,
            aborted: false,
            finish_started: false,
            total_limit: constraints.policy.max_total_bytes,
            total_bytes: constraints.byte_counter(),
        }
    }
}

impl TrzszFileWriter for NativeDownloadFileWriter {
    fn close_file(&mut self) {}

    fn file_name(&self) -> &str {
        &self.file_name
    }

    fn local_name(&self) -> &str {
        &self.local_name
    }

    fn is_dir(&self) -> bool {
        false
    }

    fn write_file(&mut self, data: &[u8]) -> Result<(), TrzszError> {
        if self.finished || self.aborted {
            return Err(TrzszError::InvalidState(format!(
                "Download writer is no longer active: {}",
                self.file_name
            )));
        }
        let mut total_bytes = self
            .total_bytes
            .lock()
            .expect("trzsz download byte counter");
        *total_bytes = total_bytes.saturating_add(data.len() as u64);
        if *total_bytes > self.total_limit {
            return Err(TrzszError::MaxTotalBytesExceeded {
                selected: *total_bytes,
                max: self.total_limit,
            });
        }
        drop(total_bytes);
        download::write_download_chunk(
            &self.state,
            &self.owner_id,
            TRZSZ_API_VERSION,
            &self.writer_id,
            data.to_vec(),
        )
    }

    fn delete_file(&mut self) -> Result<String, TrzszError> {
        if self.finished {
            download::remove_download_file(
                &self.state,
                &self.owner_id,
                TRZSZ_API_VERSION,
                self.root_path.clone(),
                self.relative_path.clone(),
            )?;
        } else if let Err(error) = self.abort_file() {
            if !(self.finish_started && matches!(error, TrzszError::HandleNotFound(_))) {
                return Err(error);
            }
            download::remove_download_file(
                &self.state,
                &self.owner_id,
                TRZSZ_API_VERSION,
                self.root_path.clone(),
                self.relative_path.clone(),
            )?;
        }
        for directory_path in self.cleanup_directories.iter().rev() {
            download::remove_download_directory(
                &self.state,
                &self.owner_id,
                TRZSZ_API_VERSION,
                self.root_path.clone(),
                directory_path.clone(),
            )?;
        }
        Ok(String::new())
    }

    fn commit_file(&mut self) -> Result<(), TrzszError> {
        for directory_path in &self.cleanup_directories {
            download::commit_download_directory(
                &self.state,
                &self.owner_id,
                TRZSZ_API_VERSION,
                self.root_path.clone(),
                directory_path.clone(),
            )?;
        }
        Ok(())
    }

    fn finish_file(&mut self) -> Result<(), TrzszError> {
        if self.finished || self.aborted {
            return Ok(());
        }
        self.finish_started = true;
        download::finish_download_file(
            &self.state,
            &self.owner_id,
            TRZSZ_API_VERSION,
            &self.writer_id,
        )?;
        self.finished = true;
        Ok(())
    }

    fn abort_file(&mut self) -> Result<(), TrzszError> {
        if self.finished || self.aborted {
            return Ok(());
        }
        download::abort_download_file(
            &self.state,
            &self.owner_id,
            TRZSZ_API_VERSION,
            &self.writer_id,
        )?;
        self.aborted = true;
        Ok(())
    }
}

#[derive(Debug)]
struct TrzszDirectoryEntry {
    path_id: u64,
    path_name: Vec<String>,
    is_dir: bool,
}

fn parse_directory_entry(raw: &str) -> Result<TrzszDirectoryEntry, TrzszError> {
    let payload: Value =
        serde_json::from_str(raw).map_err(|error| TrzszError::InvalidPath(error.to_string()))?;
    let path_name = payload
        .get("path_name")
        .and_then(Value::as_array)
        .ok_or_else(|| TrzszError::InvalidPath(format!("Invalid directory entry: {raw}")))?
        .iter()
        .map(|value| value.as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    if path_name.is_empty() {
        return Err(TrzszError::InvalidPath(format!(
            "Invalid directory entry: {raw}"
        )));
    }
    Ok(TrzszDirectoryEntry {
        path_id: payload
            .get("path_id")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        path_name,
        is_dir: payload
            .get("is_dir")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn check_duplicate_names(files: &[Box<dyn TrzszFileReader>]) -> Result<(), TrzszError> {
    let mut names = HashSet::new();
    for file in files {
        let path = file.rel_path().join("/");
        if !names.insert(path.clone()) {
            return Err(TrzszError::InvalidState(format!("Duplicate name: {path}")));
        }
    }
    Ok(())
}

fn next_collision_name(base_name: &str, attempt: usize) -> String {
    if attempt == 0 {
        base_name.to_string()
    } else {
        format!("{base_name}.{}", attempt - 1)
    }
}

fn is_retryable_collision(error: &TrzszError) -> bool {
    match error {
        TrzszError::AlreadyExists(_) => true,
        TrzszError::InvalidPath(message) => {
            message.contains("resolves to a directory")
                || message.contains("resolves to a file")
                || message.contains("Target path is a directory")
        }
        _ => false,
    }
}

fn is_cancelled_transfer(error: &TrzszError) -> bool {
    matches!(error, TrzszError::InvalidState(message) if message == "Stopped")
}

fn join_path(parts: impl IntoIterator<Item = String>) -> String {
    parts.into_iter().collect::<Vec<_>>().join("/")
}

fn base_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.trim_end_matches(['/', '\\']).to_string())
}

fn format_saved_files(file_names: &[String], dest_path: &str) -> String {
    let mut message = format!(
        "Saved {} {}",
        file_names.len(),
        if file_names.len() > 1 {
            "files/directories"
        } else {
            "file/directory"
        }
    );
    if !dest_path.is_empty() {
        message.push_str(" to ");
        message.push_str(dest_path);
    }
    let mut lines = vec![message];
    lines.extend(file_names.iter().cloned());
    lines.join("\r\n- ")
}
