impl SftpSession {
    async fn preview_text(
        &self,
        path: &str,
        extension: &str,
        mime_type: &str,
        file_size: u64,
    ) -> Result<PreviewContent, SftpError> {
        if file_size > constants::MAX_TEXT_PREVIEW_SIZE {
            return Ok(PreviewContent::TooLarge {
                size: file_size,
                max_size: constants::MAX_TEXT_PREVIEW_SIZE,
                recommend_download: true,
            });
        }
        let content = self.read_file_limited(path, file_size as usize).await?;
        let (data, encoding, confidence, has_bom) = detect_and_decode(&content);
        Ok(PreviewContent::Text {
            data,
            mime_type: Some(mime_type.to_string()),
            language: extension_to_language(extension),
            encoding,
            confidence,
            has_bom,
        })
    }

    async fn preview_image(
        &self,
        path: &str,
        size: u64,
        mime_type: &str,
    ) -> Result<PreviewContent, SftpError> {
        if size > constants::MAX_PREVIEW_SIZE {
            return Ok(PreviewContent::TooLarge {
                size,
                max_size: constants::MAX_PREVIEW_SIZE,
                recommend_download: true,
            });
        }
        const INLINE_THRESHOLD: u64 = 512 * 1024;
        if size <= INLINE_THRESHOLD {
            use base64::Engine;
            let bytes = self.read_file_limited(path, size as usize).await?;
            return Ok(PreviewContent::Image {
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
                mime_type: mime_type.to_string(),
            });
        }
        self.preview_asset(path, size, mime_type, AssetFileKind::Image)
            .await
    }

    async fn preview_asset(
        &self,
        path: &str,
        size: u64,
        mime_type: &str,
        kind: AssetFileKind,
    ) -> Result<PreviewContent, SftpError> {
        let max_size = match kind {
            AssetFileKind::Audio | AssetFileKind::Video => constants::MAX_MEDIA_PREVIEW_SIZE,
            AssetFileKind::Office => constants::MAX_OFFICE_CONVERT_SIZE,
            AssetFileKind::Image | AssetFileKind::Pdf | AssetFileKind::Font => {
                constants::MAX_PREVIEW_SIZE
            }
        };
        if size > max_size {
            return Ok(PreviewContent::TooLarge {
                size,
                max_size,
                recommend_download: true,
            });
        }
        let path = self.download_to_temp(path).await?;
        Ok(PreviewContent::AssetFile {
            path: path.to_string_lossy().to_string(),
            mime_type: mime_type.to_string(),
            kind,
        })
    }

    async fn preview_hex(
        &self,
        path: &str,
        total_size: u64,
        offset: u64,
    ) -> Result<PreviewContent, SftpError> {
        if offset >= total_size {
            return Ok(PreviewContent::Hex {
                data: String::new(),
                total_size,
                offset,
                chunk_size: 0,
                has_more: false,
            });
        }
        let bytes_to_read = constants::HEX_CHUNK_SIZE.min(total_size - offset) as usize;
        let mut file = self
            .sftp
            .open(path)
            .await
            .map_err(|error| self.map_sftp_error(error, path))?;
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(SftpError::IoError)?;
        }
        let mut buffer = vec![0u8; bytes_to_read];
        let read = file.read(&mut buffer).await.map_err(SftpError::IoError)?;
        buffer.truncate(read);
        Ok(PreviewContent::Hex {
            data: generate_hex_dump(&buffer, offset),
            total_size,
            offset,
            chunk_size: read as u64,
            has_more: offset + (read as u64) < total_size,
        })
    }

    async fn read_file_limited(&self, path: &str, max_bytes: usize) -> Result<Vec<u8>, SftpError> {
        let mut file = self
            .sftp
            .open(path)
            .await
            .map_err(|error| self.map_sftp_error(error, path))?;
        let mut content =
            Vec::with_capacity(max_bytes.min(constants::STREAMING_PREVIEW_CHUNK_SIZE));
        let mut remaining = max_bytes;
        let mut buffer = vec![0u8; constants::STREAMING_PREVIEW_CHUNK_SIZE.min(max_bytes.max(1))];
        while remaining > 0 {
            let read_len = remaining.min(buffer.len());
            let read = file
                .read(&mut buffer[..read_len])
                .await
                .map_err(SftpError::IoError)?;
            if read == 0 {
                break;
            }
            content.extend_from_slice(&buffer[..read]);
            remaining -= read;
        }
        Ok(content)
    }

    async fn download_to_temp(&self, remote_path: &str) -> Result<PathBuf, SftpError> {
        let extension = Path::new(remote_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("bin");
        let temp_dir = std::env::temp_dir().join("oxideterm-sftp-preview");
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .map_err(SftpError::IoError)?;
        let temp_path = temp_dir.join(format!("{}.{}", uuid::Uuid::new_v4(), extension));
        let mut remote_file = self
            .sftp
            .open(remote_path)
            .await
            .map_err(|error| self.map_sftp_error(error, remote_path))?;
        let mut local_file = tokio::fs::File::create(&temp_path)
            .await
            .map_err(SftpError::IoError)?;
        let mut buffer = vec![0u8; constants::STREAMING_PREVIEW_CHUNK_SIZE];
        loop {
            let read = remote_file
                .read(&mut buffer)
                .await
                .map_err(SftpError::IoError)?;
            if read == 0 {
                break;
            }
            local_file
                .write_all(&buffer[..read])
                .await
                .map_err(SftpError::IoError)?;
        }
        local_file.flush().await.map_err(SftpError::IoError)?;
        std::fs::canonicalize(&temp_path).map_err(SftpError::IoError)
    }

    async fn write_to_swap_and_rename(
        &self,
        canonical_path: &str,
        swap_path: &str,
        content: &[u8],
    ) -> Result<(), SftpError> {
        let mut file = self
            .sftp
            .open_with_flags(
                swap_path,
                OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
            )
            .await
            .map_err(|error| self.map_sftp_error(error, swap_path))?;
        file.write_all(content).await.map_err(|error| {
            SftpError::WriteError(format!("Failed to write swap file: {error}"))
        })?;
        file.flush().await.map_err(|error| {
            SftpError::WriteError(format!("Failed to flush swap file: {error}"))
        })?;
        drop(file);
        if let Err(error) = self.sftp.remove_file(canonical_path).await
            && !is_missing_file_error_message(&error.to_string())
        {
            warn!("Failed to remove existing target before SFTP rename: {error}");
        }
        match self.sftp.rename(swap_path, canonical_path).await {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = self.sftp.remove_file(swap_path).await;
                Err(SftpError::WriteError(format!(
                    "Atomic rename failed: {error}"
                )))
            }
        }
    }

    async fn write_direct(&self, canonical_path: &str, content: &[u8]) -> Result<(), SftpError> {
        let mut file = self
            .sftp
            .open_with_flags(
                canonical_path,
                OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
            )
            .await
            .map_err(|error| self.map_sftp_error(error, canonical_path))?;
        file.write_all(content)
            .await
            .map_err(|error| SftpError::WriteError(format!("Failed to write content: {error}")))?;
        file.flush()
            .await
            .map_err(|error| SftpError::WriteError(format!("Failed to flush file: {error}")))?;
        Ok(())
    }

    async fn resolve_path(&self, path: &str) -> Result<String, SftpError> {
        if path.is_empty() {
            return Ok(self.cwd.clone());
        }
        let path_to_resolve = if is_absolute_remote_path(path) {
            path.to_string()
        } else {
            join_remote_path(&self.cwd, path)
        };
        self.sftp
            .canonicalize(&path_to_resolve)
            .await
            .map_err(|error| self.map_sftp_error(error, &path_to_resolve))
    }

    async fn resolve_new_file_path(&self, path: &str) -> Result<String, SftpError> {
        let (parent, filename) = if let Some((parent, filename)) = path.rsplit_once('/') {
            let parent = if parent.is_empty() { "/" } else { parent };
            (parent, filename)
        } else {
            (self.cwd.as_str(), path)
        };
        if filename.is_empty() {
            return Err(SftpError::InvalidPath(format!(
                "missing file name in path: {path}"
            )));
        }
        let canonical_parent = self.resolve_path(parent).await?;
        Ok(join_remote_path(&canonical_parent, filename))
    }

    fn map_sftp_error(&self, error: SftpErrorInner, path: &str) -> SftpError {
        let message = error.to_string();
        let lower = message.to_lowercase();
        if lower.contains("permission denied") {
            SftpError::PermissionDenied(path.to_string())
        } else if is_missing_file_error_message(&lower) {
            SftpError::FileNotFound(path.to_string())
        } else if lower.contains("no such directory") {
            SftpError::DirectoryNotFound(path.to_string())
        } else {
            SftpError::ProtocolError(message)
        }
    }
}
