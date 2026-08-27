const RELAY_MAX_DIRECTORY_DEPTH: u32 = 64;
const RELAY_SOURCE_SAMPLE_BYTES: usize = 64 * 1024;
// Split the existing per-file memory window between the source reader and
// destination writer so a relay remains bounded without sacrificing pipelining.
const SFTP_RELAY_SIDE_MAX_INFLIGHT_BYTES: usize = SFTP_SINGLE_FILE_MAX_INFLIGHT_BYTES / 2;

#[derive(Clone)]
struct RelayFileJob {
    source_path: String,
    destination_path: String,
    write_path: String,
    total_bytes: u64,
}

fn minimum_advertised_handle_limit(source: Option<u64>, destination: Option<u64>) -> Option<u64> {
    match (source, destination) {
        (Some(source), Some(destination)) => Some(source.min(destination)),
        (Some(limit), None) | (None, Some(limit)) => Some(limit),
        (None, None) => None,
    }
}

fn relay_sibling_path(target_path: &str, role: &str, suffix: &str) -> String {
    format!("{target_path}.oxideterm-relay-{suffix}.{role}")
}

fn is_owned_relay_sibling_path(target_path: &str, sibling_path: &Path, role: &str) -> bool {
    let Some(sibling_path) = sibling_path.to_str() else {
        return false;
    };
    let prefix = format!("{target_path}.oxideterm-relay-");
    let suffix = format!(".{role}");
    let Some(token) = sibling_path
        .strip_prefix(&prefix)
        .and_then(|path| path.strip_suffix(&suffix))
    else {
        return false;
    };
    token.len() == 32 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_owned_relay_staging_path(target_path: &str, staging_path: &Path) -> bool {
    is_owned_relay_sibling_path(target_path, staging_path, "part")
}

fn validate_remote_relay_resume(
    progress: &StoredTransferProgress,
    context: &RemoteRelayProgressContext,
    source_path: &str,
    destination_path: &str,
    source_metadata: &FileAttributes,
    source_sample_sha256: &str,
) -> Result<StoredRemoteRelayProgress, SftpError> {
    let Some(relay) = progress.remote_relay.as_ref() else {
        return Err(SftpError::TransferError(
            "Persisted transfer is not a remote relay".to_string(),
        ));
    };
    let source_size = source_metadata.size.unwrap_or(0);
    let matches_request = relay.supports_resume()
        && progress.protocol == TransferProtocol::Sftp
        && progress.strategy == TransferStrategy::File
        && progress.source_path.as_path() == Path::new(source_path)
        && progress.destination_path.as_path() == Path::new(destination_path)
        && progress.total_bytes == source_size
        && progress.transferred_bytes <= source_size
        && relay.profile_id == context.profile_id
        && relay.profile_revision == context.profile_revision
        && relay.source_endpoint_id == context.source_endpoint_id
        && relay.destination_endpoint_id == context.destination_endpoint_id
        && is_owned_relay_staging_path(destination_path, &relay.staging_path)
        && relay.source_size == source_size
        && relay.source_modified == source_metadata.mtime
        && relay.source_sample_sha256 == source_sample_sha256;
    if !matches_request {
        return Err(SftpError::TransferError(
            "Remote relay source or endpoint configuration changed".to_string(),
        ));
    }
    Ok(relay.clone())
}

impl SftpSession {
    /// Removes only the exact staging sibling recorded for an interrupted relay.
    pub async fn discard_remote_relay_progress(
        &self,
        progress: &StoredTransferProgress,
    ) -> Result<(), SftpError> {
        let Some(relay) = progress.remote_relay.as_ref() else {
            return Err(SftpError::TransferError(
                "Persisted transfer is not a remote relay".to_string(),
            ));
        };
        let destination_path = progress.destination_path.to_string_lossy();
        if !is_owned_relay_staging_path(&destination_path, &relay.staging_path) {
            return Err(SftpError::TransferError(
                "Remote relay staging path is not owned by this transfer".to_string(),
            ));
        }
        if let Some(backup_path) = relay.backup_path.as_ref() {
            if !is_owned_relay_sibling_path(&destination_path, backup_path, "backup") {
                return Err(SftpError::TransferError(
                    "Remote relay backup path is not owned by this transfer".to_string(),
                ));
            }
            let backup_path = backup_path.to_string_lossy();
            if self.relay_path_exists(&backup_path).await? {
                if self.relay_path_exists(&destination_path).await? {
                    self.remove_relay_path_if_exists(&backup_path).await?;
                } else {
                    // A crash after backing up the old target must restore it
                    // before the user-owned partial staging file is discarded.
                    self.sftp
                        .rename(backup_path.as_ref(), destination_path.as_ref())
                        .await
                        .map_err(|error| self.map_sftp_error(error, &destination_path))?;
                }
            }
        }
        self.remove_relay_path_if_exists(&relay.staging_path.to_string_lossy())
            .await
    }

    /// Relays one regular file directly between two SFTP sessions.
    ///
    /// Data passes through bounded pipelined buffers only. Persistent relay
    /// metadata points at a destination-side staging file, never a local copy.
    #[allow(clippy::too_many_arguments)]
    pub async fn relay_file_to(
        &self,
        destination: &SftpSession,
        source_path: &str,
        destination_path: &str,
        disposition: RemoteRelayDisposition,
        transfer_id: &str,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
        progress_store: Arc<dyn ProgressStore>,
        relay_context: RemoteRelayProgressContext,
        resume_progress: Option<StoredTransferProgress>,
        transfer_type: TransferType,
    ) -> Result<u64, SftpError> {
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
        check_transfer_control(&transfer_manager, transfer_id).await?;

        let (canonical_source, metadata) = self.resolve_relay_source(source_path).await?;
        if !metadata.is_regular() {
            return Err(SftpError::InvalidPath(
                "Remote relay source is not a regular file".to_string(),
            ));
        }
        let canonical_destination = destination
            .resolve_new_file_path(destination_path)
            .await?;
        let source_sample_sha256 = self
            .relay_source_sample_sha256(&canonical_source, metadata.size.unwrap_or(0))
            .await?;
        if let Some(progress) = resume_progress.as_ref()
            && let Ok(relay) = validate_remote_relay_resume(
                progress,
                &relay_context,
                &canonical_source,
                &canonical_destination,
                &metadata,
                &source_sample_sha256,
            )
            && !destination
                .relay_path_exists(&relay.staging_path.to_string_lossy())
                .await?
            && self
                .relay_installed_target_matches_source(
                    destination,
                    &canonical_source,
                    &canonical_destination,
                    metadata.size.unwrap_or(0),
                )
                .await?
        {
            if let Some(backup_path) = relay.backup_path.as_ref() {
                if !is_owned_relay_sibling_path(
                    &canonical_destination,
                    backup_path,
                    "backup",
                ) {
                    return Err(SftpError::TransferError(
                        "Remote relay backup path is not owned by this transfer".to_string(),
                    ));
                }
                destination
                    .remove_relay_path_if_exists(&backup_path.to_string_lossy())
                    .await?;
            }
            progress_store.delete(transfer_id).await?;
            let completed_bytes = metadata.size.unwrap_or(0);
            send_transfer_progress(
                &progress_tx,
                transfer_id,
                &canonical_destination,
                &canonical_source,
                TransferDirection::Upload,
                completed_bytes,
                completed_bytes,
                Instant::now(),
                TransferState::Completed,
                None,
            )
            .await;
            return Ok(completed_bytes);
        }
        let resumed = resume_progress.is_some();
        let (write_path, effective_disposition, offset, mut stored_progress) =
            if let Some(mut progress) = resume_progress {
                let preparation = async {
                    let relay = validate_remote_relay_resume(
                        &progress,
                        &relay_context,
                        &canonical_source,
                        &canonical_destination,
                        &metadata,
                        &source_sample_sha256,
                    )?;
                    destination
                        .ensure_relay_final_target_state(
                            &canonical_destination,
                            relay.disposition,
                        )
                        .await?;
                    let offset = destination
                        .validated_relay_staging_offset(
                            &relay.staging_path.to_string_lossy(),
                            &progress,
                        )
                        .await?;
                    self.verify_relay_staging_prefix(
                        destination,
                        &canonical_source,
                        &relay.staging_path.to_string_lossy(),
                        offset,
                    )
                    .await?;
                    Ok::<_, SftpError>((relay, offset))
                }
                .await;
                let (relay, offset) = match preparation {
                    Ok(preparation) => preparation,
                    Err(error) => {
                        progress.mark_failed(error.to_string());
                        let _ = progress_store.save(&progress).await;
                        return Err(error);
                    }
                };
                progress.session_id = relay_context.storage_key();
                progress.total_bytes = metadata.size.unwrap_or(0);
                progress.update_progress(offset);
                progress.mark_active();
                let write_path = relay.staging_path.to_string_lossy().to_string();
                (write_path, relay.disposition, offset, progress)
            } else {
                destination
                    .ensure_relay_final_target_state(&canonical_destination, disposition)
                    .await?;
                let suffix = uuid::Uuid::new_v4().simple().to_string();
                let write_path = relay_sibling_path(&canonical_destination, "part", &suffix);
                let backup_path = (disposition == RemoteRelayDisposition::ReplaceExisting)
                    .then(|| relay_sibling_path(&canonical_destination, "backup", &suffix));
                let total_bytes = metadata.size.unwrap_or(0);
                let mut progress = StoredTransferProgress::new(
                    transfer_id.to_string(),
                    transfer_type,
                    PathBuf::from(&canonical_source),
                    PathBuf::from(&canonical_destination),
                    total_bytes,
                    relay_context.storage_key(),
                );
                progress.remote_relay = Some(StoredRemoteRelayProgress::new(
                    &relay_context,
                    PathBuf::from(&write_path),
                    backup_path.map(PathBuf::from),
                    disposition,
                    total_bytes,
                    metadata.mtime,
                    source_sample_sha256.clone(),
                ));
                (write_path, disposition, 0, progress)
            };
        // Persist before the first remote write so an application exit can always
        // identify the exact staging entry without retaining either SSH generation.
        progress_store.save(&stored_progress).await?;
        let job = RelayFileJob {
            source_path: canonical_source.clone(),
            destination_path: canonical_destination.clone(),
            write_path: write_path.clone(),
            total_bytes: metadata.size.unwrap_or(0),
        };
        let started = Instant::now();
        let transferred = self
            .relay_file_job_with_sftp(
                destination,
                self.sftp.clone(),
                destination.sftp.clone(),
                &job,
                transfer_id,
                &progress_tx,
                &transfer_manager,
                None,
                started,
                offset,
                resumed,
                true,
                Some(&progress_store),
                Some(&mut stored_progress),
            )
            .await;

        let transferred = match transferred {
            Ok(transferred) => transferred,
            Err(SftpError::TransferCancelled) => {
                destination
                    .cleanup_incomplete_relay_target(&write_path)
                    .await;
                progress_store.delete(transfer_id).await?;
                return Err(SftpError::TransferCancelled);
            }
            Err(SftpError::TransferShutdown) => {
                stored_progress.mark_paused();
                let _ = progress_store.save(&stored_progress).await;
                return Err(SftpError::TransferShutdown);
            }
            Err(error) => {
                stored_progress.mark_failed(error.to_string());
                let _ = progress_store.save(&stored_progress).await;
                return Err(error);
            }
        };

        let replacement_backup_path = stored_progress
            .remote_relay
            .as_ref()
            .and_then(|relay| relay.backup_path.as_ref())
            .map(|path| path.to_string_lossy().to_string());
        let completion_result = async {
            let completed_metadata = self
                .sftp
                .symlink_metadata(&canonical_source)
                .await
                .map_err(|error| self.map_sftp_error(error, &canonical_source))?;
            let completed_sample_sha256 = self
                .relay_source_sample_sha256(
                    &canonical_source,
                    completed_metadata.size.unwrap_or(0),
                )
                .await?;
            if completed_metadata.size != metadata.size
                || completed_metadata.mtime != metadata.mtime
                || completed_sample_sha256 != source_sample_sha256
            {
                return Err(SftpError::TransferError(
                    "Remote relay source changed before the transfer completed".to_string(),
                ));
            }

            match effective_disposition {
                RemoteRelayDisposition::CreateNew => {
                    destination
                        .install_relay_created_target(&write_path, &canonical_destination)
                        .await
                }
                RemoteRelayDisposition::ReplaceExisting => {
                    destination
                        .install_relay_replacement_with_backup(
                            &write_path,
                            &canonical_destination,
                            replacement_backup_path.as_deref(),
                        )
                        .await
                }
            }
        }
        .await;
        if let Err(error) = completion_result {
            stored_progress.mark_failed(error.to_string());
            let _ = progress_store.save(&stored_progress).await;
            return Err(error);
        }
        progress_store.delete(transfer_id).await?;

        send_transfer_progress(
            &progress_tx,
            transfer_id,
            &canonical_destination,
            &job.source_path,
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

    /// Relays a directory tree directly between two SFTP sessions.
    ///
    /// Enumeration and file work use the existing bounded directory scheduler.
    /// Symbolic links and special entries are skipped rather than followed.
    /// Restart resume is intentionally deferred until a paged per-file manifest
    /// can preserve this scheduler's bounded-memory behavior.
    #[allow(clippy::too_many_arguments)]
    pub async fn relay_dir_to(
        &self,
        destination: &SftpSession,
        source_path: &str,
        destination_path: &str,
        disposition: RemoteRelayDisposition,
        transfer_id: &str,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
        check_transfer_control(&transfer_manager, transfer_id).await?;

        let (canonical_source, metadata) = self.resolve_relay_source(source_path).await?;
        if !metadata.is_dir() {
            return Err(SftpError::InvalidPath(
                "Remote relay source is not a directory".to_string(),
            ));
        }
        let canonical_destination = destination
            .resolve_new_file_path(destination_path)
            .await?;
        let write_root = match disposition {
            RemoteRelayDisposition::CreateNew => canonical_destination.clone(),
            RemoteRelayDisposition::ReplaceExisting => relay_sibling_path(
                &canonical_destination,
                "part",
                &uuid::Uuid::new_v4().simple().to_string(),
            ),
        };
        destination.create_owned_relay_directory(&write_root).await?;

        let requested_parallelism = transfer_manager
            .as_ref()
            .map(|manager| manager.directory_parallelism())
            .unwrap_or(crate::DEFAULT_SFTP_DIRECTORY_PARALLELISM);
        let handle_limit = minimum_advertised_handle_limit(
            self.sftp.advertised_open_handle_limit(),
            destination.sftp.advertised_open_handle_limit(),
        );
        let plan = plan_directory_transfer(requested_parallelism, handle_limit);
        let (job_tx, job_rx) = directory_job_channel(plan);
        let (source_pool, destination_pool) = tokio::join!(
            self.open_directory_pool(plan.channel_count),
            destination.open_directory_pool(plan.channel_count),
        );
        let source_pool = Arc::new(source_pool);
        let destination_pool = Arc::new(destination_pool);
        let rate_limiter = Arc::new(DirectoryRateLimiter::new());
        let result = tokio::try_join!(
            self.produce_relay_jobs(
                destination,
                &canonical_source,
                &write_root,
                &canonical_destination,
                plan,
                transfer_id,
                &transfer_manager,
                job_tx,
            ),
            self.run_relay_jobs(
                destination,
                job_rx,
                plan,
                source_pool.clone(),
                destination_pool.clone(),
                rate_limiter,
                transfer_id,
                &progress_tx,
                &transfer_manager,
            ),
        )
        .map(|(_, completed)| completed);
        let (_, _) = tokio::join!(
            source_pool.close_auxiliary_sessions(),
            destination_pool.close_auxiliary_sessions(),
        );

        let completed = match result {
            Ok(completed) => completed,
            Err(error) => {
                destination
                    .cleanup_incomplete_relay_target(&write_root)
                    .await;
                return Err(error);
            }
        };
        if disposition == RemoteRelayDisposition::ReplaceExisting
            && let Err(error) = destination
                .install_relay_replacement(&write_root, &canonical_destination)
                .await
        {
            destination
                .cleanup_incomplete_relay_target(&write_root)
                .await;
            return Err(error);
        }
        Ok(completed)
    }

    async fn resolve_relay_source(
        &self,
        path: &str,
    ) -> Result<(String, FileAttributes), SftpError> {
        let candidate = if path.is_empty() {
            self.cwd.clone()
        } else if path == "~" {
            self.home.clone()
        } else if let Some(relative) = path.strip_prefix("~/") {
            join_remote_path(&self.home, relative)
        } else if is_absolute_remote_path(path) {
            path.to_string()
        } else {
            join_remote_path(&self.cwd, path)
        };
        let candidate = trim_relay_source_path(&candidate);
        let metadata = self
            .sftp
            .symlink_metadata(&candidate)
            .await
            .map_err(|error| self.map_sftp_error(error, &candidate))?;
        if metadata.is_symlink() {
            return Err(SftpError::InvalidPath(
                "Remote relay does not follow symbolic links".to_string(),
            ));
        }
        let canonical = self
            .sftp
            .canonicalize(&candidate)
            .await
            .map_err(|error| self.map_sftp_error(error, &candidate))?;
        Ok((canonical, metadata))
    }

    async fn relay_source_sample_sha256(
        &self,
        canonical_path: &str,
        total_bytes: u64,
    ) -> Result<String, SftpError> {
        let mut digest = Sha256::new();
        digest.update(total_bytes.to_le_bytes());
        let (_, _, first) = self
            .read_file_range(canonical_path, 0, RELAY_SOURCE_SAMPLE_BYTES)
            .await?;
        digest.update(first.as_slice());
        if total_bytes > RELAY_SOURCE_SAMPLE_BYTES as u64 {
            let tail_offset = total_bytes.saturating_sub(RELAY_SOURCE_SAMPLE_BYTES as u64);
            let (_, _, last) = self
                .read_file_range(canonical_path, tail_offset, RELAY_SOURCE_SAMPLE_BYTES)
                .await?;
            digest.update(last.as_slice());
        }
        Ok(format!("{:x}", digest.finalize()))
    }

    async fn verify_relay_staging_prefix(
        &self,
        destination: &SftpSession,
        source_path: &str,
        staging_path: &str,
        staged_bytes: u64,
    ) -> Result<(), SftpError> {
        if staged_bytes == 0 {
            return Ok(());
        }
        let sample_bytes = staged_bytes.min(RELAY_SOURCE_SAMPLE_BYTES as u64) as usize;
        let tail_offset = staged_bytes.saturating_sub(sample_bytes as u64);
        let (_, _, source_first) = self.read_file_range(source_path, 0, sample_bytes).await?;
        let (_, _, staged_first) = destination
            .read_file_range(staging_path, 0, sample_bytes)
            .await?;
        let (_, _, source_last) = self
            .read_file_range(source_path, tail_offset, sample_bytes)
            .await?;
        let (_, _, staged_last) = destination
            .read_file_range(staging_path, tail_offset, sample_bytes)
            .await?;
        if source_first.as_slice() != staged_first.as_slice()
            || source_last.as_slice() != staged_last.as_slice()
        {
            return Err(SftpError::TransferError(
                "Remote relay staging content no longer matches the source".to_string(),
            ));
        }
        Ok(())
    }

    async fn ensure_relay_final_target_state(
        &self,
        target_path: &str,
        disposition: RemoteRelayDisposition,
    ) -> Result<(), SftpError> {
        if disposition == RemoteRelayDisposition::ReplaceExisting {
            return Ok(());
        }
        match self.sftp.symlink_metadata(target_path).await {
            Err(error) if is_missing_file_error_message(&error.to_string()) => Ok(()),
            Err(error) => Err(self.map_sftp_error(error, target_path)),
            Ok(_) => Err(SftpError::TransferError(
                "Remote relay target appeared while the transfer was offline".to_string(),
            )),
        }
    }

    async fn relay_path_exists(&self, path: &str) -> Result<bool, SftpError> {
        match self.sftp.symlink_metadata(path).await {
            Ok(_) => Ok(true),
            Err(error) if is_missing_file_error_message(&error.to_string()) => Ok(false),
            Err(error) => Err(self.map_sftp_error(error, path)),
        }
    }

    async fn relay_installed_target_matches_source(
        &self,
        destination: &SftpSession,
        source_path: &str,
        target_path: &str,
        total_bytes: u64,
    ) -> Result<bool, SftpError> {
        let metadata = match destination.sftp.symlink_metadata(target_path).await {
            Ok(metadata) => metadata,
            Err(error) if is_missing_file_error_message(&error.to_string()) => return Ok(false),
            Err(error) => return Err(destination.map_sftp_error(error, target_path)),
        };
        if !metadata.is_regular()
            || metadata.is_symlink()
            || metadata.size.unwrap_or(0) != total_bytes
        {
            return Ok(false);
        }
        self.verify_relay_staging_prefix(destination, source_path, target_path, total_bytes)
            .await
            .map(|_| true)
            .or_else(|error| match error {
                SftpError::TransferError(_) => Ok(false),
                error => Err(error),
            })
    }

    async fn validated_relay_staging_offset(
        &self,
        staging_path: &str,
        progress: &StoredTransferProgress,
    ) -> Result<u64, SftpError> {
        let metadata = self
            .sftp
            .symlink_metadata(staging_path)
            .await
            .map_err(|error| self.map_sftp_error(error, staging_path))?;
        if !metadata.is_regular() || metadata.is_symlink() {
            return Err(SftpError::TransferError(
                "Remote relay staging target is not a regular file".to_string(),
            ));
        }
        let staging_bytes = metadata.size.unwrap_or(0);
        if staging_bytes < progress.transferred_bytes || staging_bytes > progress.total_bytes {
            return Err(SftpError::TransferError(
                "Remote relay staging size does not match persisted progress".to_string(),
            ));
        }
        // The remote file can be ahead of the throttled database checkpoint after
        // a crash. Its verified contiguous size is the safe resume offset.
        Ok(staging_bytes)
    }

    #[allow(clippy::too_many_arguments)]
    async fn produce_relay_jobs(
        &self,
        destination: &SftpSession,
        source_root: &str,
        write_root: &str,
        destination_root: &str,
        plan: DirectoryTransferPlan,
        transfer_id: &str,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
        job_tx: tokio::sync::mpsc::Sender<RelayFileJob>,
    ) -> Result<(), SftpError> {
        let mut pending = VecDeque::from([(
            source_root.to_string(),
            write_root.to_string(),
            destination_root.to_string(),
            0,
            false,
        )]);
        let mut scans = stream::FuturesUnordered::new();

        loop {
            while scans.len() < plan.worker_count
                && let Some((source_dir, write_dir, destination_dir, depth, create_write_dir)) =
                    pending.pop_front()
            {
                scans.push(async move {
                    check_transfer_control(transfer_manager, transfer_id).await?;
                    if depth >= RELAY_MAX_DIRECTORY_DEPTH {
                        return Err(SftpError::TransferError(format!(
                            "remote relay recursion depth {RELAY_MAX_DIRECTORY_DEPTH} reached"
                        )));
                    }
                    if create_write_dir {
                        // Child creation stays on the destination's node-backed
                        // session and completes before any child file is queued.
                        destination
                            .create_owned_relay_directory(&write_dir)
                            .await?;
                    }
                    let entries = self.list_tree_entries_resolved(&source_dir).await?;
                    Ok::<_, SftpError>((write_dir, destination_dir, depth, entries))
                });
            }

            let Some(scan) = scans.next().await else {
                break;
            };
            let (write_dir, destination_dir, depth, entries) = scan?;
            for entry in entries {
                if entry.is_symlink {
                    debug!("Skipping symbolic link during remote SFTP relay");
                    continue;
                }
                let write_path = join_remote_path(&write_dir, &entry.name);
                let destination_path = join_remote_path(&destination_dir, &entry.name);
                match entry.file_type {
                    FileType::Directory => {
                        pending.push_back((
                            entry.path,
                            write_path,
                            destination_path,
                            depth + 1,
                            true,
                        ));
                    }
                    FileType::File => {
                        job_tx
                            .send(RelayFileJob {
                                source_path: entry.path,
                                destination_path,
                                write_path,
                                total_bytes: entry.size,
                            })
                            .await
                            .map_err(|_| SftpError::TransferCancelled)?;
                    }
                    FileType::Symlink | FileType::Unknown => {
                        debug!("Skipping unsupported entry during remote SFTP relay");
                    }
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_relay_jobs(
        &self,
        destination: &SftpSession,
        job_rx: tokio::sync::mpsc::Receiver<RelayFileJob>,
        plan: DirectoryTransferPlan,
        source_pool: Arc<DirectorySftpPool>,
        destination_pool: Arc<DirectorySftpPool>,
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
            let source_pool = source_pool.clone();
            let destination_pool = destination_pool.clone();
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
                let started = Instant::now();
                let transferred = self
                    .relay_file_job_with_sftp(
                        destination,
                        source_pool.session_for_worker(worker_index),
                        destination_pool.session_for_worker(worker_index),
                        &job,
                        transfer_id,
                        progress_tx,
                        transfer_manager,
                        Some(rate_limiter.as_ref()),
                        started,
                        0,
                        false,
                        false,
                        None,
                        None,
                    )
                    .await?;
                send_transfer_progress(
                    progress_tx,
                    transfer_id,
                    &job.destination_path,
                    &job.source_path,
                    TransferDirection::Upload,
                    job.total_bytes,
                    transferred,
                    started,
                    TransferState::Completed,
                    None,
                )
                .await;
                Ok::<u64, SftpError>(1)
            }
        })
        .buffer_unordered(plan.worker_count)
        .try_fold(0, |sum, count| async move { Ok(sum + count) })
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn relay_file_job_with_sftp(
        &self,
        destination: &SftpSession,
        source_sftp: Arc<RusshSftpSession>,
        destination_sftp: Arc<RusshSftpSession>,
        job: &RelayFileJob,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
        directory_rate_limiter: Option<&DirectoryRateLimiter>,
        started: Instant,
        offset: u64,
        resume_existing: bool,
        preserve_incomplete: bool,
        progress_store: Option<&Arc<dyn ProgressStore>>,
        stored_progress: Option<&mut StoredTransferProgress>,
    ) -> Result<u64, SftpError> {
        let source_file = source_sftp
            .open(&job.source_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &job.source_path))?;
        let destination_flags = if resume_existing {
            OpenFlags::WRITE
        } else {
            OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE
        };
        let destination_file = destination_sftp
            .open_with_flags(&job.write_path, destination_flags)
            .await
            .map_err(|error| destination.map_sftp_error(error, &job.destination_path))?;

        // From this point the relay owns the new destination entry. Fresh
        // directory jobs remove it on error; resumable files retain it.
        let result = self
            .relay_open_files(
                destination,
                source_file,
                destination_file,
                job,
                transfer_id,
                progress_tx,
                transfer_manager,
                directory_rate_limiter,
                started,
                offset,
                progress_store,
                stored_progress,
            )
            .await;
        if result.is_err() && !preserve_incomplete {
            destination
                .cleanup_incomplete_relay_target(&job.write_path)
                .await;
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn relay_open_files(
        &self,
        destination: &SftpSession,
        source_file: russh_sftp::client::fs::File,
        destination_file: russh_sftp::client::fs::File,
        job: &RelayFileJob,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
        directory_rate_limiter: Option<&DirectoryRateLimiter>,
        started: Instant,
        offset: u64,
        progress_store: Option<&Arc<dyn ProgressStore>>,
        mut stored_progress: Option<&mut StoredTransferProgress>,
    ) -> Result<u64, SftpError> {
        let mut source_reader = source_file.into_pipelined_downloader_for_range(
            offset,
            Some(job.total_bytes),
            AdaptiveChunkSizer::MAX_CHUNK,
            SFTP_DOWNLOAD_MAX_REQUESTS,
            SFTP_RELAY_SIDE_MAX_INFLIGHT_BYTES,
        );
        let mut destination_writer = destination_file.into_pipelined_uploader(
            offset,
            AdaptiveChunkSizer::MAX_CHUNK,
            SFTP_UPLOAD_MAX_REQUESTS,
            SFTP_RELAY_SIDE_MAX_INFLIGHT_BYTES,
        );
        let mut transferred = offset;
        let mut last_progress = Instant::now();
        let mut last_persist = Instant::now();
        loop {
            check_transfer_control(transfer_manager, transfer_id).await?;
            let Some(chunk) = source_reader
                .next_chunk()
                .await
                .map_err(|error| self.map_sftp_error(error, &job.source_path))?
            else {
                break;
            };
            if chunk.offset != transferred {
                return Err(SftpError::ProtocolError(
                    "Remote relay reader returned a non-contiguous chunk".to_string(),
                ));
            }
            if let Some(rate_limiter) = directory_rate_limiter {
                rate_limiter
                    .throttle(chunk.data.len(), transfer_manager)
                    .await;
            }
            let scheduled = destination_writer
                .write_all_chunk(&chunk.data)
                .await
                .map_err(|error| destination.map_sftp_error(error, &job.destination_path))?;
            transferred = chunk.offset.saturating_add(scheduled as u64);
            if directory_rate_limiter.is_none() {
                let _ = throttle_transfer(transferred, started, transfer_manager).await;
            }
            if last_progress.elapsed().as_millis() >= 200 {
                if let Some(progress) = stored_progress.as_deref_mut() {
                    progress.update_progress(transferred);
                    if last_persist.elapsed() >= SFTP_PROGRESS_PERSIST_INTERVAL
                        && let Some(progress_store) = progress_store
                    {
                        // Keep database I/O outside the per-chunk hot path.
                        progress_store.save(progress).await?;
                        last_persist = Instant::now();
                    }
                }
                send_transfer_progress(
                    progress_tx,
                    transfer_id,
                    &job.destination_path,
                    &job.source_path,
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

        let source_result = source_reader.shutdown().await;
        let destination_result = destination_writer.shutdown().await;
        source_result.map_err(|error| self.map_sftp_error(error, &job.source_path))?;
        destination_result
            .map_err(|error| destination.map_sftp_error(error, &job.destination_path))?;
        if let Some(progress) = stored_progress {
            progress.update_progress(transferred);
            if let Some(progress_store) = progress_store {
                progress_store.save(progress).await?;
            }
        }
        Ok(transferred)
    }

    async fn create_owned_relay_directory(&self, path: &str) -> Result<(), SftpError> {
        self.sftp
            .create_dir(path)
            .await
            .map_err(|error| self.map_sftp_error(error, path))
    }

    async fn install_relay_created_target(
        &self,
        staged_path: &str,
        target_path: &str,
    ) -> Result<(), SftpError> {
        self.ensure_relay_final_target_state(target_path, RemoteRelayDisposition::CreateNew)
            .await?;
        self.sftp
            .rename(staged_path, target_path)
            .await
            .map_err(|error| self.map_sftp_error(error, target_path))
    }

    async fn install_relay_replacement(
        &self,
        staged_path: &str,
        target_path: &str,
    ) -> Result<(), SftpError> {
        self.install_relay_replacement_with_backup(staged_path, target_path, None)
            .await
    }

    async fn install_relay_replacement_with_backup(
        &self,
        staged_path: &str,
        target_path: &str,
        persisted_backup_path: Option<&str>,
    ) -> Result<(), SftpError> {
        let generated_backup_path;
        let backup_path = if let Some(backup_path) = persisted_backup_path {
            if !is_owned_relay_sibling_path(target_path, Path::new(backup_path), "backup") {
                return Err(SftpError::TransferError(
                    "Remote relay backup path is not owned by this transfer".to_string(),
                ));
            }
            backup_path
        } else {
            generated_backup_path = relay_sibling_path(
                target_path,
                "backup",
                &uuid::Uuid::new_v4().simple().to_string(),
            );
            &generated_backup_path
        };
        match self.sftp.symlink_metadata(target_path).await {
            Err(error) if is_missing_file_error_message(&error.to_string()) => {
                self
                    .sftp
                    .rename(staged_path, target_path)
                    .await
                    .map_err(|error| self.map_sftp_error(error, target_path))?;
                return self.remove_relay_path_if_exists(backup_path).await;
            }
            Err(error) => return Err(self.map_sftp_error(error, target_path)),
            Ok(_) => {}
        }

        if self.relay_path_exists(backup_path).await? {
            return Err(SftpError::TransferError(
                "Remote relay backup already exists".to_string(),
            ));
        }
        self.sftp
            .rename(target_path, backup_path)
            .await
            .map_err(|error| self.map_sftp_error(error, target_path))?;
        if let Err(error) = self.sftp.rename(staged_path, target_path).await {
            let rollback = self.sftp.rename(backup_path, target_path).await;
            if let Err(rollback_error) = rollback {
                return Err(SftpError::TransferError(format!(
                    "Failed to install remote relay target: {error}; rollback failed: {rollback_error}"
                )));
            }
            return Err(self.map_sftp_error(error, target_path));
        }

        if let Err(error) = self.remove_relay_path_if_exists(backup_path).await {
            if persisted_backup_path.is_some() {
                return Err(error);
            }
            warn!("Failed to remove remote relay backup after successful replacement: {error}");
        }
        Ok(())
    }

    async fn cleanup_incomplete_relay_target(&self, path: &str) {
        if let Err(error) = self.remove_relay_path_if_exists(path).await {
            warn!("Failed to remove incomplete remote SFTP relay target: {error}");
        }
    }

    async fn remove_relay_path_if_exists(&self, path: &str) -> Result<(), SftpError> {
        let metadata = match self.sftp.symlink_metadata(path).await {
            Ok(metadata) => metadata,
            Err(error) if is_missing_file_error_message(&error.to_string()) => return Ok(()),
            Err(error) => return Err(self.map_sftp_error(error, path)),
        };
        if !metadata.is_dir() || metadata.is_symlink() {
            return self
                .sftp
                .remove_file(path)
                .await
                .map_err(|error| self.map_sftp_error(error, path));
        }
        let plan = plan_directory_transfer(
            crate::DEFAULT_SFTP_DIRECTORY_PARALLELISM,
            self.sftp.advertised_open_handle_limit(),
        );
        // Cleanup shares the same bounded post-order scheduler as user deletion;
        // it never follows links and does not outlive this relay operation.
        self.delete_directory_tree_resolved(path, plan.worker_count)
            .await
            .map(|_| ())
    }
}

fn trim_relay_source_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let is_windows_root = bytes.len() == 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');
    if path == "/" || is_windows_root {
        path.to_string()
    } else {
        path.trim_end_matches(['/', '\\']).to_string()
    }
}

#[cfg(test)]
mod relay_tests {
    use super::*;

    #[test]
    fn relay_pipeline_stays_within_the_existing_single_file_window() {
        assert!(AdaptiveChunkSizer::MAX_CHUNK <= SFTP_RELAY_SIDE_MAX_INFLIGHT_BYTES);
        assert!(
            SFTP_RELAY_SIDE_MAX_INFLIGHT_BYTES.saturating_mul(2)
                <= SFTP_SINGLE_FILE_MAX_INFLIGHT_BYTES
        );
    }

    #[test]
    fn relay_directory_uses_the_tighter_endpoint_handle_limit() {
        assert_eq!(minimum_advertised_handle_limit(Some(32), Some(12)), Some(12));
        assert_eq!(minimum_advertised_handle_limit(Some(8), None), Some(8));
        assert_eq!(minimum_advertised_handle_limit(None, None), None);
    }

    #[test]
    fn relay_staging_paths_are_siblings_of_the_requested_target() {
        assert_eq!(
            relay_sibling_path("/srv/data", "part", "fixed"),
            "/srv/data.oxideterm-relay-fixed.part"
        );
    }

    #[test]
    fn relay_resume_accepts_only_its_owned_staging_sibling() {
        assert!(is_owned_relay_staging_path(
            "/srv/data",
            Path::new("/srv/data.oxideterm-relay-0123456789abcdef0123456789abcdef.part")
        ));
        assert!(is_owned_relay_sibling_path(
            "/srv/data",
            Path::new("/srv/data.oxideterm-relay-0123456789abcdef0123456789abcdef.backup"),
            "backup"
        ));
        assert!(!is_owned_relay_staging_path(
            "/srv/data",
            Path::new("/srv/other.oxideterm-relay-0123456789abcdef0123456789abcdef.part")
        ));
        assert!(!is_owned_relay_staging_path(
            "/srv/data",
            Path::new("/srv/data.oxideterm-relay-not-a-transfer.part")
        ));
    }

    #[test]
    fn relay_source_trimming_preserves_remote_roots() {
        assert_eq!(trim_relay_source_path("/"), "/");
        assert_eq!(trim_relay_source_path("C:/"), "C:/");
        assert_eq!(trim_relay_source_path("/srv/data///"), "/srv/data");
    }
}
