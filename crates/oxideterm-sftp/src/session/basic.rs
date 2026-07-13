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
            session_id,
            cwd,
        })
    }

    async fn open_sibling_sftp(&self) -> Result<RusshSftpSession, SftpError> {
        open_russh_sftp_session(&self.channel_factory).await
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
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
