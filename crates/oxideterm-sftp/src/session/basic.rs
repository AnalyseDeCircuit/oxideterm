impl SftpSession {
    pub async fn new<O>(connection: O, session_id: String) -> Result<Self, SftpError>
    where
        O: SftpChannelOpener,
    {
        info!("Opening SFTP subsystem for session {session_id}");
        // Store an erased channel factory so directory transfers can open
        // short-lived sibling SFTP channels without changing the public opener API.
        let channel_factory: SftpChannelFactory = Arc::new(move || {
            let connection = connection.clone();
            Box::pin(async move { connection.open_sftp_channel().await })
        });
        let sftp = open_russh_sftp_session(&channel_factory).await?;
        let cwd = sftp
            .canonicalize(".")
            .await
            .map_err(|error| SftpError::ProtocolError(error.to_string()))?;
        info!("SFTP subsystem opened for session {session_id}");
        Ok(Self {
            sftp: Arc::new(sftp),
            channel_factory,
            _connection_owner: None,
            single_channel_transport: false,
            session_id,
            home: cwd.clone(),
            cwd,
        })
    }

    pub fn with_connection_owner(mut self, owner: Arc<dyn Send + Sync>) -> Self {
        // Short-lived dedicated transports must survive until the SFTP channel
        // and every sibling channel opened by this session have been dropped.
        self._connection_owner = Some(owner);
        self
    }

    /// Prevents directory batching from opening sibling SSH channels.
    pub fn with_single_channel_transport(mut self) -> Self {
        self.single_channel_transport = true;
        self
    }

    async fn open_sibling_sftp(&self) -> Result<RusshSftpSession, SftpError> {
        open_russh_sftp_session(&self.channel_factory).await
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Returns the SFTP server's initial directory even after the file manager changes cwd.
    pub fn home(&self) -> &str {
        &self.home
    }

    pub fn set_cwd(&mut self, path: String) {
        self.cwd = path;
    }

    pub async fn canonicalize(&self, path: &str) -> Result<String, SftpError> {
        self.resolve_path(path).await
    }

    pub async fn list_dir(
        &self,
        path: &str,
        filter: Option<ListFilter>,
    ) -> Result<Vec<FileInfo>, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        self.list_dir_resolved(&canonical_path, filter).await
    }

    pub async fn list_dir_with_cwd(
        &self,
        path: &str,
        filter: Option<ListFilter>,
    ) -> Result<(String, Vec<FileInfo>), SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        let entries = self.list_dir_resolved(&canonical_path, filter).await?;
        Ok((canonical_path, entries))
    }

    async fn list_dir_resolved(
        &self,
        canonical_path: &str,
        filter: Option<ListFilter>,
    ) -> Result<Vec<FileInfo>, SftpError> {
        debug!("Listing SFTP directory: {canonical_path}");
        let read_dir = self
            .sftp
            .read_dir(canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, canonical_path))?;
        let mut entries = Vec::new();

        for entry in read_dir {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            // SFTP names are single components; rejecting separators keeps recursive
            // downloads contained beneath their selected local destination.
            validate_remote_entry_name(&name)?;
            if filter.as_ref().is_some_and(|f| !f.show_hidden) && name.starts_with('.') {
                continue;
            }

            let full_path = join_remote_path(canonical_path, &name);
            let metadata = entry.metadata();
            let entry_file_type = file_type_from_attrs(&metadata);
            let (symlink_target, target_file_type) = if entry_file_type == FileType::Symlink {
                let symlink_target = self.sftp.read_link(&full_path).await.ok();
                let target_file_type = self
                    .sftp
                    .metadata(&full_path)
                    .await
                    .ok()
                    .map(|target_metadata| file_type_from_attrs(&target_metadata));
                (symlink_target, target_file_type)
            } else {
                (None, None)
            };
            let file_type = classify_list_entry_file_type(entry_file_type, target_file_type);
            entries.push(FileInfo {
                name,
                path: full_path,
                file_type,
                size: metadata.size.unwrap_or(0),
                modified: metadata.mtime.map(|mtime| mtime as i64).unwrap_or(0),
                permissions: metadata
                    .permissions
                    .map(|permissions| format!("{:o}", permissions & 0o777))
                    .unwrap_or_else(|| "000".to_string()),
                owner: metadata.uid.map(|uid| uid.to_string()),
                group: metadata.gid.map(|gid| gid.to_string()),
                is_symlink: entry_file_type == FileType::Symlink,
                symlink_target,
            });
        }

        if let Some(pattern) = filter.as_ref().and_then(|filter| filter.pattern.as_ref())
            && let Ok(glob_pattern) = glob::Pattern::new(pattern)
        {
            entries.retain(|entry| glob_pattern.matches(&entry.name));
        }

        let sort_order = filter
            .as_ref()
            .map(|filter| filter.sort)
            .unwrap_or_default();
        sort_entries(&mut entries, sort_order);
        Ok(entries)
    }

    async fn list_tree_entries_resolved(
        &self,
        canonical_path: &str,
    ) -> Result<Vec<RemoteTreeEntry>, SftpError> {
        let read_dir = self
            .sftp
            .read_dir(canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, canonical_path))?;
        let mut entries = Vec::new();

        for entry in read_dir {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            // Recursive operations need wire metadata only. Avoid canonicalizing,
            // sorting, and resolving symlink targets on their latency-sensitive path.
            validate_remote_entry_name(&name)?;
            let metadata = entry.metadata();
            let file_type = file_type_from_attrs(&metadata);
            entries.push(RemoteTreeEntry {
                path: join_remote_path(canonical_path, &name),
                name,
                file_type,
                size: metadata.size.unwrap_or(0),
                is_symlink: file_type == FileType::Symlink,
            });
        }

        Ok(entries)
    }

    pub async fn stat(&self, path: &str) -> Result<FileInfo, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        let metadata = self
            .sftp
            .metadata(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
        let name = Path::new(&canonical_path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let file_type = file_type_from_attrs(&metadata);
        let symlink_target = if file_type == FileType::Symlink {
            self.sftp.read_link(&canonical_path).await.ok()
        } else {
            None
        };
        Ok(FileInfo {
            name,
            path: canonical_path,
            file_type,
            size: metadata.size.unwrap_or(0),
            modified: metadata.mtime.map(|mtime| mtime as i64).unwrap_or(0),
            permissions: metadata
                .permissions
                .map(|permissions| format!("{:o}", permissions & 0o777))
                .unwrap_or_else(|| "000".to_string()),
            owner: metadata.uid.map(|uid| uid.to_string()),
            group: metadata.gid.map(|gid| gid.to_string()),
            is_symlink: file_type == FileType::Symlink,
            symlink_target,
        })
    }

    pub async fn read_file_bytes(&self, path: &str) -> Result<Vec<u8>, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        let metadata = self
            .sftp
            .metadata(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
        if metadata.is_dir() {
            return Err(SftpError::DirectoryNotFound(canonical_path));
        }
        self.read_file_limited(
            &canonical_path,
            metadata.size.unwrap_or(0).try_into().unwrap_or(usize::MAX),
        )
        .await
    }

    /// Reads one bounded range without allocating for the complete remote file.
    pub async fn read_file_range(
        &self,
        path: &str,
        offset: u64,
        maximum_bytes: usize,
    ) -> Result<(String, u64, Zeroizing<Vec<u8>>), SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        let metadata = self
            .sftp
            .metadata(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
        if metadata.is_dir() {
            return Err(SftpError::DirectoryNotFound(canonical_path));
        }
        let total_size = metadata.size.unwrap_or(0);
        if offset > total_size {
            return Err(SftpError::InvalidPath(format!(
                "offset exceeds remote file size: {canonical_path}"
            )));
        }
        let read_limit = (total_size - offset)
            .min(u64::try_from(maximum_bytes).unwrap_or(u64::MAX));
        let mut file = self
            .sftp
            .open(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(SftpError::IoError)?;
        }
        let mut bytes = Zeroizing::new(Vec::with_capacity(
            usize::try_from(read_limit).unwrap_or(maximum_bytes),
        ));
        file.take(read_limit)
            .read_to_end(&mut bytes)
            .await
            .map_err(SftpError::IoError)?;
        Ok((canonical_path, total_size, bytes))
    }

    /// Resolves an existing path or a new file beneath its canonical parent.
    pub async fn canonicalize_write_target(&self, path: &str) -> Result<String, SftpError> {
        match self.resolve_path(path).await {
            Ok(path) => Ok(path),
            Err(_) => self.resolve_new_file_path(path).await,
        }
    }

    pub async fn write_content(
        &self,
        path: &str,
        content: &[u8],
    ) -> Result<WriteContentResult, SftpError> {
        let canonical_path = match self.resolve_path(path).await {
            Ok(path) => path,
            Err(_) => self.resolve_new_file_path(path).await?,
        };
        let swap_path = swap_path(&canonical_path);
        match self
            .write_to_swap_and_rename(&canonical_path, &swap_path, content)
            .await
        {
            Ok(()) => Ok(WriteContentResult { atomic_write: true }),
            Err(error) => {
                let error_string = error.to_string();
                let recoverable = matches!(error, SftpError::PermissionDenied(_))
                    || error_string.contains(".oxswp")
                    || error_string.contains("Atomic rename failed");
                if !recoverable {
                    return Err(error);
                }
                warn!(
                    "Atomic SFTP write failed for {canonical_path} ({error_string}), falling back to direct overwrite"
                );
                self.write_direct(&canonical_path, content).await?;
                Ok(WriteContentResult {
                    atomic_write: false,
                })
            }
        }
    }

    /// Replaces a user configuration file without following-path or metadata loss.
    pub async fn replace_config_content(
        &self,
        path: &str,
        content: &[u8],
    ) -> Result<WriteContentResult, SftpError> {
        let canonical_path = match self.resolve_path(path).await {
            Ok(path) => path,
            Err(_) => self.resolve_new_file_path(path).await?,
        };
        let metadata = self.sftp.metadata(&canonical_path).await.ok();
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let swap_path = format!("{canonical_path}.oxideterm-{suffix}.tmp");
        let backup_path = format!("{canonical_path}.oxideterm-{suffix}.bak");

        self.write_direct(&swap_path, content).await?;
        let written = self.read_file_limited(&swap_path, content.len()).await?;
        if written != content {
            let _ = self.sftp.remove_file(&swap_path).await;
            return Err(SftpError::WriteError(format!(
                "Remote configuration verification failed for {canonical_path}"
            )));
        }
        if let Some(metadata) = metadata.as_ref() {
            let preserved = FileAttributes {
                uid: metadata.uid,
                gid: metadata.gid,
                permissions: metadata.permissions,
                ..FileAttributes::empty()
            };
            if let Err(error) = self.sftp.set_metadata(&swap_path, preserved).await {
                let _ = self.sftp.remove_file(&swap_path).await;
                return Err(SftpError::WriteError(format!(
                    "Failed to preserve metadata for {canonical_path}: {error}"
                )));
            }
            if let Err(error) = self.sftp.rename(&canonical_path, &backup_path).await {
                let _ = self.sftp.remove_file(&swap_path).await;
                return Err(self.map_sftp_error(error, &canonical_path));
            }
        }

        if let Err(error) = self.sftp.rename(&swap_path, &canonical_path).await {
            let rollback_error = if metadata.is_some() {
                self.sftp
                    .rename(&backup_path, &canonical_path)
                    .await
                    .err()
            } else {
                None
            };
            let _ = self.sftp.remove_file(&swap_path).await;
            if let Some(rollback_error) = rollback_error {
                return Err(SftpError::WriteError(format!(
                    "Failed to replace {canonical_path}: {error}; rollback failed: {rollback_error}. The original file remains at {backup_path}"
                )));
            }
            return Err(SftpError::WriteError(format!(
                "Failed to replace {canonical_path}: {error}"
            )));
        }
        if metadata.is_some() {
            let _ = self.sftp.remove_file(&backup_path).await;
        }
        Ok(WriteContentResult { atomic_write: true })
    }
}

async fn open_russh_sftp_session(
    channel_factory: &SftpChannelFactory,
) -> Result<RusshSftpSession, SftpError> {
    let channel = channel_factory().await?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|error| {
            SftpError::SubsystemNotAvailable(format!(
                "Failed to request SFTP subsystem: {error}"
            ))
        })?;
    let (reader, writer) = channel.into_stream().into_split();
    let config = russh_sftp::client::Config {
        // The live SFTP session owns this shared budget from queue admission
        // through acknowledgement, matching the existing upload in-flight cap.
        max_outbound_inflight_bytes: SFTP_SINGLE_FILE_MAX_INFLIGHT_BYTES,
        ..Default::default()
    };
    RusshSftpSession::new_owned_with_config(reader, RusshOwnedSftpWriter(writer), config)
        .await
        .map_err(|error| SftpError::SubsystemNotAvailable(error.to_string()))
}

struct RusshOwnedSftpWriter(russh::ChannelStreamWriter<russh::client::Msg>);

impl russh_sftp::client::OwnedSftpWriter for RusshOwnedSftpWriter {
    async fn write_owned(&mut self, data: bytes::Bytes) -> std::io::Result<()> {
        self.0.write_bytes(data).await
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        self.0.shutdown().await
    }
}
