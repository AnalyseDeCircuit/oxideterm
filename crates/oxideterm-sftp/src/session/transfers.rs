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
        let mut offset = match tokio::fs::metadata(local_path).await {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(SftpError::IoError(error)),
        };

        let stored = progress_store.load(&transfer_id).await?;
        if stored
            .as_ref()
            .is_some_and(|progress| progress.total_bytes != total_bytes)
            || offset > total_bytes
        {
            progress_store.delete(&transfer_id).await?;
            tokio::fs::File::create(local_path)
                .await
                .map_err(SftpError::IoError)?;
            offset = 0;
        }

        let mut stored_progress = StoredTransferProgress::new(
            transfer_id.clone(),
            TransferType::Download,
            PathBuf::from(&canonical_remote),
            PathBuf::from(local_path),
            total_bytes,
            self.session_id.clone(),
        );
        stored_progress.transferred_bytes = offset;
        progress_store.save(&stored_progress).await?;

        let result = self
            .download_file_resume_inner(
                &DownloadFileJob {
                    remote_path: canonical_remote.clone(),
                    local_path: local_path.to_string(),
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
        let mut jobs = Vec::new();
        self.collect_download_jobs_depth(&canonical_remote, local_path, 0, &mut jobs)
            .await?;
        self.run_download_jobs(jobs, transfer_id, &progress_tx, &transfer_manager)
            .await
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
        let mut jobs = Vec::new();
        let mut dirs = vec![canonical_remote.clone()];
        self.collect_upload_jobs_depth(local_path, &canonical_remote, 0, &mut dirs, &mut jobs)
            .await?;

        let mut seen = HashSet::new();
        for dir in dirs {
            if seen.insert(dir.clone()) {
                let _ = self.mkdir(&dir).await;
            }
        }

        self.run_upload_jobs(jobs, transfer_id, &progress_tx, &transfer_manager)
            .await
    }

    async fn collect_download_jobs_depth(
        &self,
        remote_path: &str,
        local_path: &str,
        depth: u32,
        jobs: &mut Vec<DownloadFileJob>,
    ) -> Result<(), SftpError> {
        const MAX_DEPTH: u32 = 64;
        if depth >= MAX_DEPTH {
            return Err(SftpError::TransferError(format!(
                "download recursion depth {MAX_DEPTH} reached at {remote_path}"
            )));
        }
        let mut stack = VecDeque::from([(remote_path.to_string(), local_path.to_string(), depth)]);
        while let Some((remote_dir, local_dir, current_depth)) = stack.pop_back() {
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
                    jobs.push(DownloadFileJob {
                        remote_path: entry.path,
                        local_path: local_entry,
                        total_bytes: entry.size,
                    });
                }
            }
        }
        Ok(())
    }

    async fn collect_upload_jobs_depth(
        &self,
        local_path: &str,
        remote_path: &str,
        depth: u32,
        all_remote_dirs: &mut Vec<String>,
        jobs: &mut Vec<UploadFileJob>,
    ) -> Result<(), SftpError> {
        const MAX_DEPTH: u32 = 64;
        if depth >= MAX_DEPTH {
            return Err(SftpError::TransferError(format!(
                "upload recursion depth {MAX_DEPTH} reached at {local_path}"
            )));
        }
        let mut stack =
            VecDeque::from([(PathBuf::from(local_path), remote_path.to_string(), depth)]);
        while let Some((local_dir, remote_dir, current_depth)) = stack.pop_back() {
            if current_depth >= MAX_DEPTH {
                return Err(SftpError::TransferError(format!(
                    "upload recursion depth {MAX_DEPTH} reached at {}",
                    local_dir.display()
                )));
            }
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
                    all_remote_dirs.push(remote_entry.clone());
                    stack.push_back((local_entry, remote_entry, current_depth + 1));
                } else if metadata.is_file() {
                    jobs.push(UploadFileJob {
                        local_path: local_entry.to_string_lossy().to_string(),
                        remote_path: remote_entry,
                        total_bytes: metadata.len(),
                    });
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
        jobs: Vec<DownloadFileJob>,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let parallelism = transfer_manager
            .as_ref()
            .map(|manager| manager.directory_parallelism())
            .unwrap_or(1)
            .clamp(1, crate::MAX_SFTP_DIRECTORY_PARALLELISM);
        if parallelism <= 1
            || transfer_manager
                .as_ref()
                .is_some_and(|m| m.speed_limit_bps() > 0)
        {
            for job in &jobs {
                self.download_file_inner(job, transfer_id, progress_tx, transfer_manager)
                    .await?;
            }
            return Ok(jobs.len() as u64);
        }
        stream::iter(jobs)
            .map(|job| async move {
                self.download_file_inner(&job, transfer_id, progress_tx, transfer_manager)
                    .await?;
                Ok::<u64, SftpError>(1)
            })
            .buffer_unordered(parallelism)
            .try_fold(0, |sum, count| async move { Ok(sum + count) })
            .await
    }

    async fn run_upload_jobs(
        &self,
        jobs: Vec<UploadFileJob>,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let parallelism = transfer_manager
            .as_ref()
            .map(|manager| manager.directory_parallelism())
            .unwrap_or(1)
            .clamp(1, crate::MAX_SFTP_DIRECTORY_PARALLELISM);
        if parallelism <= 1
            || transfer_manager
                .as_ref()
                .is_some_and(|m| m.speed_limit_bps() > 0)
        {
            for job in &jobs {
                self.upload_file_inner(job, transfer_id, progress_tx, transfer_manager)
                    .await?;
            }
            return Ok(jobs.len() as u64);
        }
        stream::iter(jobs)
            .map(|job| async move {
                self.upload_file_inner(&job, transfer_id, progress_tx, transfer_manager)
                    .await?;
                Ok::<u64, SftpError>(1)
            })
            .buffer_unordered(parallelism)
            .try_fold(0, |sum, count| async move { Ok(sum + count) })
            .await
    }

    async fn download_file_inner(
        &self,
        job: &DownloadFileJob,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> Result<(), SftpError> {
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
        let mut local_file = tokio::fs::File::create(&job.local_path)
            .await
            .map_err(SftpError::IoError)?;
        let mut remote_reader = remote_file.into_pipelined_downloader_for_range(
            0,
            Some(job.total_bytes),
            AdaptiveChunkSizer::MAX_CHUNK,
            SFTP_DOWNLOAD_MAX_REQUESTS,
            SFTP_DOWNLOAD_MAX_INFLIGHT_BYTES,
        );
        let started = Instant::now();
        let mut transferred = 0u64;
        let mut last_progress = Instant::now();
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
                // Pipelined reads are emitted in order, but seeking keeps the
                // local file correct if a server short-read forces a restart.
                local_file
                    .seek(std::io::SeekFrom::Start(chunk.offset))
                    .await
                    .map_err(SftpError::IoError)?;
            }
            local_file
                .write_all(&chunk.data)
                .await
                .map_err(SftpError::IoError)?;
            transferred = chunk.offset.saturating_add(read as u64);
            throttle_transfer(transferred, started, transfer_manager).await;
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
        let mut local_file = tokio::fs::File::open(&job.local_path)
            .await
            .map_err(SftpError::IoError)?;
        let remote_file = self
            .sftp
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
            SFTP_UPLOAD_MAX_INFLIGHT_BYTES,
        );
        let mut buffer = vec![0u8; AdaptiveChunkSizer::MAX_CHUNK];
        let mut chunk_sizer = AdaptiveChunkSizer::new();
        let started = Instant::now();
        let mut transferred = 0u64;
        let mut last_progress = Instant::now();
        loop {
            check_transfer_control(transfer_manager, transfer_id).await?;
            let chunk_size = chunk_sizer.chunk_size();
            let read = local_file
                .read(&mut buffer[..chunk_size])
                .await
                .map_err(SftpError::IoError)?;
            if read == 0 {
                break;
            }
            let scheduled = remote_writer
                .write_all_chunk(&buffer[..read])
                .await
                .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
            transferred = transferred.saturating_add(scheduled as u64);
            chunk_sizer.record(scheduled);
            throttle_transfer(transferred, started, transfer_manager).await;
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
        let mut local_file = if offset > 0 {
            tokio::fs::OpenOptions::new()
                .write(true)
                .open(&job.local_path)
                .await
                .map_err(SftpError::IoError)?
        } else {
            tokio::fs::File::create(&job.local_path)
                .await
                .map_err(SftpError::IoError)?
        };
        if offset > 0 {
            local_file
                .seek(std::io::SeekFrom::End(0))
                .await
                .map_err(SftpError::IoError)?;
        }
        let mut remote_reader = remote_file.into_pipelined_downloader_for_range(
            offset,
            Some(job.total_bytes),
            AdaptiveChunkSizer::MAX_CHUNK,
            SFTP_DOWNLOAD_MAX_REQUESTS,
            SFTP_DOWNLOAD_MAX_INFLIGHT_BYTES,
        );
        let started = Instant::now();
        let mut transferred = offset;
        let mut last_progress = Instant::now();
        let mut last_persist = Instant::now();
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
                // Preserve sparse correctness if a short-read causes the
                // pipelined reader to restart at a non-speculative offset.
                local_file
                    .seek(std::io::SeekFrom::Start(chunk.offset))
                    .await
                    .map_err(SftpError::IoError)?;
            }
            local_file
                .write_all(&chunk.data)
                .await
                .map_err(SftpError::IoError)?;
            transferred = chunk.offset.saturating_add(read as u64);
            throttle_transfer(
                transferred.saturating_sub(offset),
                started,
                transfer_manager,
            )
            .await;
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
            SFTP_UPLOAD_MAX_INFLIGHT_BYTES,
        );
        let mut buffer = vec![0u8; AdaptiveChunkSizer::MAX_CHUNK];
        let mut chunk_sizer = AdaptiveChunkSizer::new();
        let started = Instant::now();
        let mut transferred = offset;
        let mut last_progress = Instant::now();
        let mut last_persist = Instant::now();
        loop {
            check_transfer_control(transfer_manager, transfer_id).await?;
            let chunk_size = chunk_sizer.chunk_size();
            let read = local_file
                .read(&mut buffer[..chunk_size])
                .await
                .map_err(SftpError::IoError)?;
            if read == 0 {
                break;
            }
            let scheduled = remote_writer
                .write_all_chunk(&buffer[..read])
                .await
                .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
            transferred = transferred.saturating_add(scheduled as u64);
            chunk_sizer.record(scheduled);
            throttle_transfer(
                transferred.saturating_sub(offset),
                started,
                transfer_manager,
            )
            .await;
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
