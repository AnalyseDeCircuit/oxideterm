struct DirectorySftpPool {
    sessions: Vec<Arc<RusshSftpSession>>,
}

impl DirectorySftpPool {
    fn new(primary: Arc<RusshSftpSession>) -> Self {
        Self {
            sessions: vec![primary],
        }
    }

    fn push_auxiliary(&mut self, session: RusshSftpSession) {
        self.sessions.push(Arc::new(session));
    }

    fn session_for_worker(&self, worker_index: usize) -> Arc<RusshSftpSession> {
        self.sessions[worker_index % self.sessions.len()].clone()
    }

    async fn close_auxiliary_sessions(&self) {
        // The first entry is the long-lived browser session; only close the
        // temporary channels opened for this directory transfer.
        for session in self.sessions.iter().skip(1) {
            let _ = session.close().await;
        }
    }
}

fn validate_download_resume_progress(
    progress: &StoredTransferProgress,
    transfer_id: &str,
    remote_path: &str,
    local_path: &str,
    total_bytes: u64,
) -> Result<(), SftpError> {
    let matches_transfer = progress.transfer_id == transfer_id
        && progress.transfer_type == TransferType::Download
        && progress.protocol == TransferProtocol::Sftp
        && progress.strategy == TransferStrategy::File
        && progress.source_path == PathBuf::from(remote_path)
        && progress.destination_path == PathBuf::from(local_path)
        && progress.total_bytes == total_bytes
        && progress.is_incomplete();
    if matches_transfer {
        Ok(())
    } else {
        Err(SftpError::TransferError(
            "Persisted download progress does not match the requested resume".to_string(),
        ))
    }
}

async fn open_local_download_file(
    local_path: &str,
    disposition: LocalDownloadDisposition,
) -> Result<tokio::fs::File, SftpError> {
    match disposition {
        LocalDownloadDisposition::CreateNew => tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(local_path)
            .await
            .map_err(SftpError::IoError),
        LocalDownloadDisposition::ReplaceExisting => {
            if let Ok(metadata) = tokio::fs::symlink_metadata(local_path).await
                && metadata.file_type().is_symlink()
            {
                return Err(SftpError::InvalidPath(
                    "Refusing to follow a symbolic link while replacing a download target"
                        .to_string(),
                ));
            }
            tokio::fs::File::create(local_path)
                .await
                .map_err(SftpError::IoError)
        }
        LocalDownloadDisposition::ResumeVerified => tokio::fs::OpenOptions::new()
            .write(true)
            .open(local_path)
            .await
            .map_err(SftpError::IoError),
    }
}

impl SftpSession {
    pub async fn download_file(
        &self,
        remote_path: &str,
        local_path: &str,
        transfer_id: &str,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
        let canonical_remote = self.resolve_path(remote_path).await?;
        let remote_info = self.stat(&canonical_remote).await?;
        self.download_file_inner(
            &DownloadFileJob {
                remote_path: canonical_remote,
                local_path: local_path.to_string(),
                total_bytes: remote_info.size,
            },
            transfer_id,
            &progress_tx,
            &transfer_manager,
        )
        .await?;
        Ok(remote_info.size)
    }

    pub async fn upload_file(
        &self,
        local_path: &str,
        remote_path: &str,
        transfer_id: &str,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
        let metadata = tokio::fs::metadata(local_path)
            .await
            .map_err(SftpError::IoError)?;
        let canonical_remote = self.resolve_new_file_path(remote_path).await?;
        self.upload_file_inner(
            &UploadFileJob {
                local_path: local_path.to_string(),
                remote_path: canonical_remote,
                total_bytes: metadata.len(),
            },
            transfer_id,
            &progress_tx,
            &transfer_manager,
        )
        .await?;
        Ok(metadata.len())
    }

    pub async fn download_with_resume(
        &self,
        remote_path: &str,
        local_path: &str,
        disposition: LocalDownloadDisposition,
        progress_store: Arc<dyn ProgressStore>,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
        transfer_id: Option<String>,
    ) -> Result<u64, SftpError> {
        let transfer_id = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(&transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id.clone());
        let canonical_remote = self.resolve_path(remote_path).await?;
        let remote_info = self.stat(&canonical_remote).await?;
        let total_bytes = remote_info.size;
        let stored = progress_store.load(&transfer_id).await?;
        let offset = if disposition == LocalDownloadDisposition::ResumeVerified {
            let progress = stored.as_ref().ok_or_else(|| {
                SftpError::TransferError(
                    "Download resume requires a matching persisted transfer".to_string(),
                )
            })?;
            validate_download_resume_progress(
                progress,
                &transfer_id,
                &canonical_remote,
                local_path,
                total_bytes,
            )?;
            let metadata = tokio::fs::symlink_metadata(local_path)
                .await
                .map_err(SftpError::IoError)?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(SftpError::InvalidPath(
                    "Download resume target is not a regular file".to_string(),
                ));
            }
            if metadata.len() > total_bytes {
                return Err(SftpError::TransferError(
                    "Download resume target exceeds the remote file size".to_string(),
                ));
            }
            metadata.len()
        } else {
            if stored.is_some() {
                return Err(SftpError::TransferError(
                    "Fresh download unexpectedly reused an existing transfer identifier"
                        .to_string(),
                ));
            }
            0
        };

        let mut stored_progress = stored.unwrap_or_else(|| {
            StoredTransferProgress::new(
                transfer_id.clone(),
                TransferType::Download,
                PathBuf::from(&canonical_remote),
                PathBuf::from(local_path),
                total_bytes,
                self.session_id.clone(),
            )
        });
        stored_progress.mark_active();
        stored_progress.transferred_bytes = offset;

        let result = self
            .download_file_resume_inner(
                &DownloadFileJob {
                    remote_path: canonical_remote.clone(),
                    local_path: local_path.to_string(),
                    total_bytes,
                },
                &transfer_id,
                offset,
                disposition,
                &progress_tx,
                &transfer_manager,
                progress_store.clone(),
                stored_progress,
            )
            .await;

        match result {
            Ok(transferred) => {
                progress_store.delete(&transfer_id).await?;
                Ok(transferred)
            }
            Err(SftpError::TransferCancelled) => {
                progress_store.delete(&transfer_id).await?;
                Err(SftpError::TransferCancelled)
            }
            Err(error) => {
                if let Ok(Some(mut progress)) = progress_store.load(&transfer_id).await {
                    progress.mark_failed(error.to_string());
                    let _ = progress_store.save(&progress).await;
                }
                Err(error)
            }
        }
    }

    pub async fn upload_with_resume(
        &self,
        local_path: &str,
        remote_path: &str,
        progress_store: Arc<dyn ProgressStore>,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
        transfer_id: Option<String>,
    ) -> Result<u64, SftpError> {
        let transfer_id = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(&transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id.clone());
        let canonical_remote = self.resolve_new_file_path(remote_path).await?;
        let temp_remote = format!("{canonical_remote}.oxide-part");
        let metadata = tokio::fs::metadata(local_path)
            .await
            .map_err(SftpError::IoError)?;
        let total_bytes = metadata.len();

        let stored = progress_store
            .list_incomplete(&self.session_id)
            .await?
            .into_iter()
            .find(|progress| {
                progress.transfer_type == TransferType::Upload
                    && progress.source_path == PathBuf::from(local_path)
                    && progress.destination_path == PathBuf::from(&canonical_remote)
            });
        if let Some(progress) = stored.as_ref()
            && progress.total_bytes != total_bytes
        {
            progress_store.delete(&progress.transfer_id).await?;
            let _ = self.delete(&temp_remote).await;
        }

        let offset = match self.stat(&temp_remote).await {
            Ok(info) if info.size >= total_bytes => {
                self.replace_remote_file(&temp_remote, &canonical_remote)
                    .await?;
                progress_store.delete(&transfer_id).await?;
                return Ok(total_bytes);
            }
            Ok(info) => info.size,
            Err(_) => 0,
        };

        let mut stored_progress = StoredTransferProgress::new(
            transfer_id.clone(),
            TransferType::Upload,
            PathBuf::from(local_path),
            PathBuf::from(&canonical_remote),
            total_bytes,
            self.session_id.clone(),
        );
        stored_progress.transferred_bytes = offset;
        progress_store.save(&stored_progress).await?;

        let result = self
            .upload_file_resume_inner(
                &UploadFileJob {
                    local_path: local_path.to_string(),
                    remote_path: temp_remote.clone(),
                    total_bytes,
                },
                &transfer_id,
                offset,
                &progress_tx,
                &transfer_manager,
                progress_store.clone(),
                stored_progress,
            )
            .await;

        match result {
            Ok(transferred) => {
                self.replace_remote_file(&temp_remote, &canonical_remote)
                    .await?;
                progress_store.delete(&transfer_id).await?;
                Ok(transferred)
            }
            Err(error) if should_retry_upload_without_temporary_file(&error, &temp_remote, offset) => {
                let _ = self.delete(&temp_remote).await;
                progress_store.delete(&transfer_id).await?;

                // Some virtual SFTP gateways allow the requested destination
                // but reject resumable sibling files. Limit the compatibility
                // fallback to transfers that have not written any bytes.
                self.upload_file_inner(
                    &UploadFileJob {
                        local_path: local_path.to_string(),
                        remote_path: canonical_remote,
                        total_bytes,
                    },
                    &transfer_id,
                    &progress_tx,
                    &transfer_manager,
                )
                .await?;
                Ok(total_bytes)
            }
            Err(SftpError::TransferCancelled) => {
                let _ = self.delete(&temp_remote).await;
                progress_store.delete(&transfer_id).await?;
                Err(SftpError::TransferCancelled)
            }
            Err(error) => {
                if let Ok(Some(mut progress)) = progress_store.load(&transfer_id).await {
                    progress.mark_failed(error.to_string());
                    let _ = progress_store.save(&progress).await;
                }
                Err(error)
            }
        }
    }

    pub async fn download_dir(
        &self,
        remote_path: &str,
        local_path: &str,
        transfer_id: &str,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
        let canonical_remote = self.resolve_path(remote_path).await?;
        tokio::fs::create_dir_all(local_path)
            .await
            .map_err(SftpError::IoError)?;
        let plan = self.directory_transfer_plan(&transfer_manager);
        let (job_tx, job_rx) = directory_job_channel(plan);
        let pool = Arc::new(self.open_directory_pool(plan.channel_count).await);
        let rate_limiter = Arc::new(DirectoryRateLimiter::new());
        let result = tokio::try_join!(
            self.produce_download_jobs(
                &canonical_remote,
                local_path,
                transfer_id,
                &transfer_manager,
                job_tx,
            ),
            self.run_download_jobs(
                job_rx,
                plan,
                pool.clone(),
                rate_limiter,
                transfer_id,
                &progress_tx,
                &transfer_manager,
            )
        )
        .map(|(_, completed)| completed);
        pool.close_auxiliary_sessions().await;
        result
    }

    pub async fn upload_dir(
        &self,
        local_path: &str,
        remote_path: &str,
        transfer_id: &str,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
        let canonical_remote = if is_absolute_remote_path(remote_path) {
            remote_path.to_string()
        } else {
            join_remote_path(&self.cwd, remote_path)
        };
        let plan = self.directory_transfer_plan(&transfer_manager);
        let (job_tx, job_rx) = directory_job_channel(plan);
        let pool = Arc::new(self.open_directory_pool(plan.channel_count).await);
        let rate_limiter = Arc::new(DirectoryRateLimiter::new());
        let result = tokio::try_join!(
            self.produce_upload_jobs(
                local_path,
                &canonical_remote,
                transfer_id,
                &transfer_manager,
                job_tx,
            ),
            self.run_upload_jobs(
                job_rx,
                plan,
                pool.clone(),
                rate_limiter,
                transfer_id,
                &progress_tx,
                &transfer_manager,
            )
        )
        .map(|(_, completed)| completed);
        pool.close_auxiliary_sessions().await;
        result
    }

    fn directory_transfer_plan(
        &self,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> DirectoryTransferPlan {
        let requested_parallelism = transfer_manager
            .as_ref()
            .map(|manager| manager.directory_parallelism())
            .unwrap_or(1);
        plan_directory_transfer(
            requested_parallelism,
            self.sftp.advertised_open_handle_limit(),
        )
    }

    async fn produce_download_jobs(
        &self,
        remote_path: &str,
        local_path: &str,
        transfer_id: &str,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
        job_tx: tokio::sync::mpsc::Sender<DownloadFileJob>,
    ) -> Result<(), SftpError> {
        const MAX_DEPTH: u32 = 64;
        let mut stack = VecDeque::from([(remote_path.to_string(), local_path.to_string(), 0)]);
        while let Some((remote_dir, local_dir, current_depth)) = stack.pop_back() {
            check_transfer_control(transfer_manager, transfer_id).await?;
            if current_depth >= MAX_DEPTH {
                return Err(SftpError::TransferError(format!(
                    "download recursion depth {MAX_DEPTH} reached at {remote_dir}"
                )));
            }
            let entries = self
                .list_dir(
                    &remote_dir,
                    Some(ListFilter {
                        show_hidden: true,
                        pattern: None,
                        sort: SortOrder::Name,
                    }),
                )
                .await?;
            for entry in entries {
                let local_entry = join_local_path(&local_dir, &entry.name);
                if entry.file_type == FileType::Directory {
                    tokio::fs::create_dir_all(&local_entry)
                        .await
                        .map_err(SftpError::IoError)?;
                    stack.push_back((entry.path, local_entry, current_depth + 1));
                } else {
                    // A bounded send lets workers start immediately while keeping
                    // large directory trees from accumulating in memory.
                    job_tx
                        .send(DownloadFileJob {
                            remote_path: entry.path,
                            local_path: local_entry,
                            total_bytes: entry.size,
                        })
                        .await
                        .map_err(|_| SftpError::TransferCancelled)?;
                }
            }
        }
        Ok(())
    }

    async fn produce_upload_jobs(
        &self,
        local_path: &str,
        remote_path: &str,
        transfer_id: &str,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
        job_tx: tokio::sync::mpsc::Sender<UploadFileJob>,
    ) -> Result<(), SftpError> {
        const MAX_DEPTH: u32 = 64;
        let mut stack =
            VecDeque::from([(PathBuf::from(local_path), remote_path.to_string(), 0)]);
        while let Some((local_dir, remote_dir, current_depth)) = stack.pop_back() {
            check_transfer_control(transfer_manager, transfer_id).await?;
            if current_depth >= MAX_DEPTH {
                return Err(SftpError::TransferError(format!(
                    "upload recursion depth {MAX_DEPTH} reached at {}",
                    local_dir.display()
                )));
            }
            // Parent directories are created before their file jobs enter the
            // queue. Existing-directory responses remain intentionally benign.
            let _ = self.mkdir(&remote_dir).await;
            let mut entries = tokio::fs::read_dir(&local_dir)
                .await
                .map_err(SftpError::IoError)?;
            while let Some(entry) = entries.next_entry().await.map_err(SftpError::IoError)? {
                let name = entry.file_name().to_string_lossy().to_string();
                let local_entry = entry.path();
                let remote_entry = join_remote_path(&remote_dir, &name);
                let metadata = match tokio::fs::symlink_metadata(&local_entry).await {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        warn!(
                            "Skipping inaccessible local entry {:?}: {error}",
                            local_entry
                        );
                        continue;
                    }
                };
                if metadata.file_type().is_symlink() {
                    warn!(
                        "Skipping local symlink during SFTP upload: {:?}",
                        local_entry
                    );
                    continue;
                }
                if metadata.is_dir() {
                    stack.push_back((local_entry, remote_entry, current_depth + 1));
                } else if metadata.is_file() {
                    job_tx
                        .send(UploadFileJob {
                            local_path: local_entry.to_string_lossy().to_string(),
                            remote_path: remote_entry,
                            total_bytes: metadata.len(),
                        })
                        .await
                        .map_err(|_| SftpError::TransferCancelled)?;
                } else {
                    warn!(
                        "Skipping special local entry during SFTP upload: {:?}",
                        local_entry
                    );
                }
            }
        }
        Ok(())
    }

    async fn run_download_jobs(
        &self,
        job_rx: tokio::sync::mpsc::Receiver<DownloadFileJob>,
        plan: DirectoryTransferPlan,
        pool: Arc<DirectorySftpPool>,
        rate_limiter: Arc<DirectoryRateLimiter>,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let bulk_lane = Arc::new(tokio::sync::Semaphore::new(plan.bulk_lane_workers));
        stream::unfold(job_rx, |mut receiver| async move {
            receiver.recv().await.map(|job| (job, receiver))
        })
            .enumerate()
            .map(|(worker_index, job)| {
                let pool = pool.clone();
                let bulk_lane = bulk_lane.clone();
                let rate_limiter = rate_limiter.clone();
                async move {
                    let _bulk_permit = match plan.classify_size(job.total_bytes) {
                        DirectoryJobClass::Compact => None,
                        DirectoryJobClass::Bulk => Some(
                            bulk_lane
                                .acquire()
                                .await
                                .map_err(|error| SftpError::TransferError(error.to_string()))?,
                        ),
                    };
                    let sftp = pool.session_for_worker(worker_index);
                    self.download_file_inner_with_sftp(
                        sftp,
                        &job,
                        transfer_id,
                        progress_tx,
                        transfer_manager,
                        Some(rate_limiter.as_ref()),
                    )
                    .await?;
                    Ok::<u64, SftpError>(1)
                }
            })
            .buffer_unordered(plan.worker_count)
            .try_fold(0, |sum, count| async move { Ok(sum + count) })
            .await
    }

    async fn run_upload_jobs(
        &self,
        job_rx: tokio::sync::mpsc::Receiver<UploadFileJob>,
        plan: DirectoryTransferPlan,
        pool: Arc<DirectorySftpPool>,
        rate_limiter: Arc<DirectoryRateLimiter>,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let bulk_lane = Arc::new(tokio::sync::Semaphore::new(plan.bulk_lane_workers));
        stream::unfold(job_rx, |mut receiver| async move {
            receiver.recv().await.map(|job| (job, receiver))
        })
            .enumerate()
            .map(|(worker_index, job)| {
                let pool = pool.clone();
                let bulk_lane = bulk_lane.clone();
                let rate_limiter = rate_limiter.clone();
                async move {
                    let _bulk_permit = match plan.classify_size(job.total_bytes) {
                        DirectoryJobClass::Compact => None,
                        DirectoryJobClass::Bulk => Some(
                            bulk_lane
                                .acquire()
                                .await
                                .map_err(|error| SftpError::TransferError(error.to_string()))?,
                        ),
                    };
                    let sftp = pool.session_for_worker(worker_index);
                    self.upload_file_inner_with_sftp(
                        sftp,
                        &job,
                        transfer_id,
                        progress_tx,
                        transfer_manager,
                        Some(rate_limiter.as_ref()),
                    )
                    .await?;
                    Ok::<u64, SftpError>(1)
                }
            })
            .buffer_unordered(plan.worker_count)
            .try_fold(0, |sum, count| async move { Ok(sum + count) })
            .await
    }

    async fn open_directory_pool(&self, channel_count: usize) -> DirectorySftpPool {
        let mut pool = DirectorySftpPool::new(self.sftp.clone());
        let auxiliary_count = channel_count.saturating_sub(1);
        // Auxiliary sessions are owned by this directory batch. Opening them
        // concurrently shortens startup without tying them to any file future.
        let mut openings = stream::iter(0..auxiliary_count)
            .map(|_| self.open_sibling_sftp())
            .buffer_unordered(auxiliary_count.max(1));
        while let Some(result) = openings.next().await {
            match result {
                Ok(session) => pool.push_auxiliary(session),
                Err(error) => {
                    warn!(
                        "Failed to open auxiliary SFTP channel for directory transfer: {error}"
                    );
                }
            }
        }
        pool
    }

    async fn download_file_inner(
        &self,
        job: &DownloadFileJob,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> Result<(), SftpError> {
        self.download_file_inner_with_sftp(
            self.sftp.clone(),
            job,
            transfer_id,
            progress_tx,
            transfer_manager,
            None,
        )
        .await
    }

    async fn download_file_inner_with_sftp(
        &self,
        sftp: Arc<RusshSftpSession>,
        job: &DownloadFileJob,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
        directory_rate_limiter: Option<&DirectoryRateLimiter>,
    ) -> Result<(), SftpError> {
        let remote_file = sftp
            .open(&job.remote_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
        if let Some(parent) = Path::new(&job.local_path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(SftpError::IoError)?;
        }
        // Recursive and direct downloads must never overwrite an existing local entry.
        let mut local_file =
            open_local_download_file(&job.local_path, LocalDownloadDisposition::CreateNew).await?;
        let mut remote_reader = remote_file.into_pipelined_downloader_for_range(
            0,
            Some(job.total_bytes),
            AdaptiveChunkSizer::MAX_CHUNK,
            SFTP_DOWNLOAD_MAX_REQUESTS,
            SFTP_SINGLE_FILE_MAX_INFLIGHT_BYTES,
        );
        let started = Instant::now();
        let mut transferred = 0u64;
        let mut last_progress = Instant::now();
        let mut diagnostics = LocalSftpDiagnostics::new();
        loop {
            check_transfer_control(transfer_manager, transfer_id).await?;
            let shared_throttle_sleep = if let Some(rate_limiter) = directory_rate_limiter {
                let remaining = job.total_bytes.saturating_sub(transferred);
                let reserved_bytes = usize::try_from(remaining)
                    .unwrap_or(usize::MAX)
                    .min(remote_reader.diagnostic_snapshot().window.target_chunk_len);
                if reserved_bytes == 0 {
                    std::time::Duration::ZERO
                } else {
                    // Reserve the batch budget before issuing more reads so each
                    // pipelined file cannot create its own full-speed burst.
                    rate_limiter
                        .throttle(reserved_bytes, transfer_manager)
                        .await
                }
            } else {
                std::time::Duration::ZERO
            };
            let Some(chunk) = remote_reader
                .next_chunk()
                .await
                .map_err(|error| self.map_sftp_error(error, &job.remote_path))?
            else {
                break;
            };
            let read = chunk.data.len();
            if chunk.offset != transferred {
                // Pipelined reads are emitted in order, but keep the local
                // offset defensive if a future recovery path emits a gap.
                let seek_started = Instant::now();
                local_file
                    .seek(std::io::SeekFrom::Start(chunk.offset))
                    .await
                    .map_err(SftpError::IoError)?;
                diagnostics.record_local_seek(seek_started.elapsed());
            }
            let write_started = Instant::now();
            local_file
                .write_all(&chunk.data)
                .await
                .map_err(SftpError::IoError)?;
            diagnostics.record_local_write(read, write_started.elapsed());
            transferred = chunk.offset.saturating_add(read as u64);
            let throttle_sleep = if directory_rate_limiter.is_some() {
                shared_throttle_sleep
            } else {
                throttle_transfer(transferred, started, transfer_manager).await
            };
            diagnostics.record_throttle_sleep(throttle_sleep);
            if diagnostics.should_log() {
                emit_local_sftp_diagnostics(format_download_diagnostics(
                    transferred,
                    job.total_bytes,
                    remote_reader.diagnostic_snapshot(),
                    &diagnostics,
                ));
            }
            if last_progress.elapsed().as_millis() >= 200 {
                send_transfer_progress(
                    progress_tx,
                    transfer_id,
                    &job.remote_path,
                    &job.local_path,
                    TransferDirection::Download,
                    job.total_bytes,
                    transferred,
                    started,
                    TransferState::InProgress,
                    None,
                )
                .await;
                last_progress = Instant::now();
            }
        }
        remote_reader
            .shutdown()
            .await
            .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
        local_file.flush().await.map_err(SftpError::IoError)?;
        send_transfer_progress(
            progress_tx,
            transfer_id,
            &job.remote_path,
            &job.local_path,
            TransferDirection::Download,
            job.total_bytes,
            transferred,
            started,
            TransferState::Completed,
            None,
        )
        .await;
        Ok(())
    }

    async fn upload_file_inner(
        &self,
        job: &UploadFileJob,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> Result<(), SftpError> {
        self.upload_file_inner_with_sftp(
            self.sftp.clone(),
            job,
            transfer_id,
            progress_tx,
            transfer_manager,
            None,
        )
        .await
    }

    async fn upload_file_inner_with_sftp(
        &self,
        sftp: Arc<RusshSftpSession>,
        job: &UploadFileJob,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
        directory_rate_limiter: Option<&DirectoryRateLimiter>,
    ) -> Result<(), SftpError> {
        let mut local_file = tokio::fs::File::open(&job.local_path)
            .await
            .map_err(SftpError::IoError)?;
        let remote_file = sftp
            .open_with_flags(
                &job.remote_path,
                OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
            )
            .await
            .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
        let mut remote_writer = remote_file.into_pipelined_uploader(
            0,
            AdaptiveChunkSizer::MAX_CHUNK,
            SFTP_UPLOAD_MAX_REQUESTS,
            SFTP_SINGLE_FILE_MAX_INFLIGHT_BYTES,
        );
        let mut buffer = vec![0u8; AdaptiveChunkSizer::MAX_CHUNK];
        let started = Instant::now();
        let mut transferred = 0u64;
        let mut last_progress = Instant::now();
        let mut diagnostics = LocalSftpDiagnostics::new();
        loop {
            check_transfer_control(transfer_manager, transfer_id).await?;
            let chunk_size = remote_writer.target_chunk_len();
            let read_started = Instant::now();
            let read = local_file
                .read(&mut buffer[..chunk_size])
                .await
                .map_err(SftpError::IoError)?;
            diagnostics.record_local_read(read, read_started.elapsed());
            if read == 0 {
                break;
            }
            let shared_throttle_sleep = if let Some(rate_limiter) = directory_rate_limiter {
                // Upload workers reserve the shared batch budget before queuing
                // bytes so parallel files cannot each consume the full limit.
                rate_limiter.throttle(read, transfer_manager).await
            } else {
                std::time::Duration::ZERO
            };
            let scheduled = remote_writer
                .write_all_chunk(&buffer[..read])
                .await
                .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
            transferred = transferred.saturating_add(scheduled as u64);
            let throttle_sleep = if directory_rate_limiter.is_some() {
                shared_throttle_sleep
            } else {
                throttle_transfer(transferred, started, transfer_manager).await
            };
            diagnostics.record_throttle_sleep(throttle_sleep);
            if diagnostics.should_log() {
                emit_local_sftp_diagnostics(format_upload_diagnostics(
                    transferred,
                    job.total_bytes,
                    remote_writer.diagnostic_snapshot(),
                    &diagnostics,
                ));
            }
            if last_progress.elapsed().as_millis() >= 200 {
                send_transfer_progress(
                    progress_tx,
                    transfer_id,
                    &job.remote_path,
                    &job.local_path,
                    TransferDirection::Upload,
                    job.total_bytes,
                    transferred,
                    started,
                    TransferState::InProgress,
                    None,
                )
                .await;
                last_progress = Instant::now();
            }
        }
        remote_writer
            .shutdown()
            .await
            .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
        send_transfer_progress(
            progress_tx,
            transfer_id,
            &job.remote_path,
            &job.local_path,
            TransferDirection::Upload,
            job.total_bytes,
            transferred,
            started,
            TransferState::Completed,
            None,
        )
        .await;
        Ok(())
    }

    async fn download_file_resume_inner(
        &self,
        job: &DownloadFileJob,
        transfer_id: &str,
        offset: u64,
        disposition: LocalDownloadDisposition,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
        progress_store: Arc<dyn ProgressStore>,
        mut stored_progress: StoredTransferProgress,
    ) -> Result<u64, SftpError> {
        let remote_file = self
            .sftp
            .open(&job.remote_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
        if let Some(parent) = Path::new(&job.local_path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(SftpError::IoError)?;
        }
        let mut local_file = open_local_download_file(&job.local_path, disposition).await?;
        if offset > 0 {
            local_file
                .seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(SftpError::IoError)?;
        }
        // Persist only after the destination was safely opened. A create-new collision
        // must not leave a resumable record pointing at somebody else's file.
        progress_store.save(&stored_progress).await?;
        let mut remote_reader = remote_file.into_pipelined_downloader_for_range(
            offset,
            Some(job.total_bytes),
            AdaptiveChunkSizer::MAX_CHUNK,
            SFTP_DOWNLOAD_MAX_REQUESTS,
            SFTP_SINGLE_FILE_MAX_INFLIGHT_BYTES,
        );
        let started = Instant::now();
        let mut transferred = offset;
        let mut last_progress = Instant::now();
        let mut last_persist = Instant::now();
        let mut diagnostics = LocalSftpDiagnostics::new();
        loop {
            check_transfer_control(transfer_manager, transfer_id).await?;
            let Some(chunk) = remote_reader
                .next_chunk()
                .await
                .map_err(|error| self.map_sftp_error(error, &job.remote_path))?
            else {
                break;
            };
            let read = chunk.data.len();
            if chunk.offset != transferred {
                // Preserve local-file correctness if a future recovery path
                // emits a non-contiguous offset during a resumed transfer.
                let seek_started = Instant::now();
                local_file
                    .seek(std::io::SeekFrom::Start(chunk.offset))
                    .await
                    .map_err(SftpError::IoError)?;
                diagnostics.record_local_seek(seek_started.elapsed());
            }
            let write_started = Instant::now();
            local_file
                .write_all(&chunk.data)
                .await
                .map_err(SftpError::IoError)?;
            diagnostics.record_local_write(read, write_started.elapsed());
            transferred = chunk.offset.saturating_add(read as u64);
            let throttle_sleep = throttle_transfer(
                transferred.saturating_sub(offset),
                started,
                transfer_manager,
            )
            .await;
            diagnostics.record_throttle_sleep(throttle_sleep);
            if diagnostics.should_log() {
                emit_local_sftp_diagnostics(format_download_diagnostics(
                    transferred,
                    job.total_bytes,
                    remote_reader.diagnostic_snapshot(),
                    &diagnostics,
                ));
            }
            if last_progress.elapsed().as_millis() >= 200 {
                stored_progress.update_progress(transferred);
                if last_persist.elapsed() >= SFTP_PROGRESS_PERSIST_INTERVAL {
                    // Persist resume state less often than UI progress so storage I/O
                    // cannot become part of the bulk transfer hot path.
                    progress_store.save(&stored_progress).await?;
                    last_persist = Instant::now();
                }
                send_transfer_progress(
                    progress_tx,
                    transfer_id,
                    &job.remote_path,
                    &job.local_path,
                    TransferDirection::Download,
                    job.total_bytes,
                    transferred,
                    started,
                    TransferState::InProgress,
                    None,
                )
                .await;
                last_progress = Instant::now();
            }
        }
        remote_reader
            .shutdown()
            .await
            .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
        local_file.flush().await.map_err(SftpError::IoError)?;
        stored_progress.mark_completed();
        progress_store.save(&stored_progress).await?;
        send_transfer_progress(
            progress_tx,
            transfer_id,
            &job.remote_path,
            &job.local_path,
            TransferDirection::Download,
            job.total_bytes,
            transferred,
            started,
            TransferState::Completed,
            None,
        )
        .await;
        Ok(transferred)
    }

    async fn upload_file_resume_inner(
        &self,
        job: &UploadFileJob,
        transfer_id: &str,
        offset: u64,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
        progress_store: Arc<dyn ProgressStore>,
        mut stored_progress: StoredTransferProgress,
    ) -> Result<u64, SftpError> {
        let mut local_file = tokio::fs::File::open(&job.local_path)
            .await
            .map_err(SftpError::IoError)?;
        if offset > 0 {
            local_file
                .seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(SftpError::IoError)?;
        }
        let remote_file = if offset > 0 {
            self.sftp
                .open_with_flags(&job.remote_path, OpenFlags::WRITE)
                .await
                .map_err(|error| self.map_sftp_error(error, &job.remote_path))?
        } else {
            self.sftp
                .open_with_flags(
                    &job.remote_path,
                    OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
                )
                .await
                .map_err(|error| self.map_sftp_error(error, &job.remote_path))?
        };
        let mut remote_writer = remote_file.into_pipelined_uploader(
            offset,
            AdaptiveChunkSizer::MAX_CHUNK,
            SFTP_UPLOAD_MAX_REQUESTS,
            SFTP_SINGLE_FILE_MAX_INFLIGHT_BYTES,
        );
        let mut buffer = vec![0u8; AdaptiveChunkSizer::MAX_CHUNK];
        let started = Instant::now();
        let mut transferred = offset;
        let mut last_progress = Instant::now();
        let mut last_persist = Instant::now();
        let mut diagnostics = LocalSftpDiagnostics::new();
        loop {
            check_transfer_control(transfer_manager, transfer_id).await?;
            let chunk_size = remote_writer.target_chunk_len();
            let read_started = Instant::now();
            let read = local_file
                .read(&mut buffer[..chunk_size])
                .await
                .map_err(SftpError::IoError)?;
            diagnostics.record_local_read(read, read_started.elapsed());
            if read == 0 {
                break;
            }
            let scheduled = remote_writer
                .write_all_chunk(&buffer[..read])
                .await
                .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
            transferred = transferred.saturating_add(scheduled as u64);
            let throttle_sleep = throttle_transfer(
                transferred.saturating_sub(offset),
                started,
                transfer_manager,
            )
            .await;
            diagnostics.record_throttle_sleep(throttle_sleep);
            if diagnostics.should_log() {
                emit_local_sftp_diagnostics(format_upload_diagnostics(
                    transferred,
                    job.total_bytes,
                    remote_writer.diagnostic_snapshot(),
                    &diagnostics,
                ));
            }
            if last_progress.elapsed().as_millis() >= 200 {
                stored_progress.update_progress(transferred);
                if last_persist.elapsed() >= SFTP_PROGRESS_PERSIST_INTERVAL {
                    // Persist resume state less often than UI progress so storage I/O
                    // cannot become part of the bulk transfer hot path.
                    progress_store.save(&stored_progress).await?;
                    last_persist = Instant::now();
                }
                send_transfer_progress(
                    progress_tx,
                    transfer_id,
                    &job.remote_path,
                    &job.local_path,
                    TransferDirection::Upload,
                    job.total_bytes,
                    transferred,
                    started,
                    TransferState::InProgress,
                    None,
                )
                .await;
                last_progress = Instant::now();
            }
        }
        remote_writer
            .shutdown()
            .await
            .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
        stored_progress.mark_completed();
        progress_store.save(&stored_progress).await?;
        send_transfer_progress(
            progress_tx,
            transfer_id,
            &job.remote_path,
            &job.local_path,
            TransferDirection::Upload,
            job.total_bytes,
            transferred,
            started,
            TransferState::Completed,
            None,
        )
        .await;
        Ok(transferred)
    }

    async fn replace_remote_file(
        &self,
        source_path: &str,
        target_path: &str,
    ) -> Result<(), SftpError> {
        if let Err(error) = self.sftp.remove_file(target_path).await
            && !is_missing_file_error_message(&error.to_string())
        {
            return Err(self.map_sftp_error(error, target_path));
        }
        self.sftp
            .rename(source_path, target_path)
            .await
            .map_err(|error| self.map_sftp_error(error, target_path))
    }
}

fn should_retry_upload_without_temporary_file(
    error: &SftpError,
    temporary_remote_path: &str,
    transferred_bytes: u64,
) -> bool {
    transferred_bytes == 0
        && matches!(
            error,
            SftpError::PermissionDenied(path) if path == temporary_remote_path
        )
}

#[cfg(test)]
mod transfer_safety_tests {
    use super::*;

    fn resumable_download() -> StoredTransferProgress {
        let mut progress = StoredTransferProgress::new(
            "transfer-1".to_string(),
            TransferType::Download,
            PathBuf::from("/remote/file.txt"),
            PathBuf::from("/local/file.txt"),
            42,
            "session-1".to_string(),
        );
        progress.mark_failed("network interruption".to_string());
        progress
    }

    #[tokio::test]
    async fn create_new_download_preserves_an_existing_local_file() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("existing.txt");
        std::fs::write(&destination, b"local work").unwrap();

        let error = open_local_download_file(
            &destination.to_string_lossy(),
            LocalDownloadDisposition::CreateNew,
        )
        .await
        .unwrap_err();

        assert!(matches!(error, SftpError::IoError(_)));
        assert_eq!(std::fs::read(destination).unwrap(), b"local work");
    }

    #[tokio::test]
    async fn explicit_replace_is_the_only_fresh_mode_that_truncates_a_local_file() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("existing.txt");
        std::fs::write(&destination, b"local work").unwrap();

        let file = open_local_download_file(
            &destination.to_string_lossy(),
            LocalDownloadDisposition::ReplaceExisting,
        )
        .await
        .unwrap();
        drop(file);

        assert!(std::fs::read(destination).unwrap().is_empty());
    }

    #[test]
    fn accepts_resume_only_for_the_exact_incomplete_download() {
        let progress = resumable_download();

        assert!(
            validate_download_resume_progress(
                &progress,
                "transfer-1",
                "/remote/file.txt",
                "/local/file.txt",
                42,
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_resume_when_the_local_destination_does_not_match() {
        let progress = resumable_download();

        assert!(
            validate_download_resume_progress(
                &progress,
                "transfer-1",
                "/remote/file.txt",
                "/local/unrelated.txt",
                42,
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_resume_without_an_incomplete_status() {
        let mut progress = resumable_download();
        progress.mark_completed();

        assert!(
            validate_download_resume_progress(
                &progress,
                "transfer-1",
                "/remote/file.txt",
                "/local/file.txt",
                42,
            )
            .is_err()
        );
    }

    #[test]
    fn retries_direct_upload_when_empty_temporary_file_is_denied() {
        let temporary_path = "/virtual/host/file.txt.oxide-part";
        let error = SftpError::PermissionDenied(temporary_path.to_string());

        assert!(should_retry_upload_without_temporary_file(
            &error,
            temporary_path,
            0
        ));
    }

    #[test]
    fn does_not_retry_direct_upload_after_partial_transfer() {
        let temporary_path = "/virtual/host/file.txt.oxide-part";
        let error = SftpError::PermissionDenied(temporary_path.to_string());

        assert!(!should_retry_upload_without_temporary_file(
            &error,
            temporary_path,
            1
        ));
    }

    #[test]
    fn does_not_retry_direct_upload_for_another_denied_path() {
        let error = SftpError::PermissionDenied("/virtual/host/file.txt".to_string());

        assert!(!should_retry_upload_without_temporary_file(
            &error,
            "/virtual/host/file.txt.oxide-part",
            0
        ));
    }
}
