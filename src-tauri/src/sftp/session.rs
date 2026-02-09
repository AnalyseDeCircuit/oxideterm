//! SFTP Session management
//!
//! Provides SFTP file operations over an existing SSH connection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use parking_lot::RwLock;
use russh_sftp::client::error::Error as SftpErrorInner;
use russh_sftp::client::SftpSession as RusshSftpSession;
use russh_sftp::protocol::OpenFlags;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::error::SftpError;
use super::path_utils::{is_absolute_remote_path, join_local_path, join_remote_path};
use super::progress::{ProgressStore, StoredTransferProgress, TransferType};
use super::retry::{transfer_with_retry, RetryConfig};
use super::types::*;
use super::transfer::TransferManager;
use crate::ssh::HandleController;

/// Resume context for partial transfers
#[derive(Debug, Clone)]
pub struct ResumeContext {
    /// Starting byte offset for resume
    pub offset: u64,
    /// Transfer ID for tracking
    pub transfer_id: String,
    /// Whether this is a resume (vs fresh transfer)
    pub is_resume: bool,
}

/// SFTP Session wrapper
pub struct SftpSession {
    /// russh SFTP session
    sftp: RusshSftpSession,
    /// Session ID this SFTP is associated with
    #[allow(dead_code)]
    session_id: String,
    /// Current working directory
    cwd: String,
}

impl SftpSession {
    /// Create a new SFTP session from a HandleController
    pub async fn new(
        handle_controller: HandleController,
        session_id: String,
    ) -> Result<Self, SftpError> {
        info!("Opening SFTP subsystem for session {}", session_id);

        // Open a new channel for SFTP via Handle Owner Task
        let channel = handle_controller
            .open_session_channel()
            .await
            .map_err(|e| SftpError::ChannelError(e.to_string()))?;

        // Request SFTP subsystem on the channel
        channel.request_subsystem(true, "sftp").await.map_err(|e| {
            SftpError::SubsystemNotAvailable(format!("Failed to request SFTP subsystem: {}", e))
        })?;

        // Create SFTP session from the channel stream
        let sftp = RusshSftpSession::new(channel.into_stream())
            .await
            .map_err(|e| SftpError::SubsystemNotAvailable(e.to_string()))?;

        info!("SFTP subsystem opened for session {}", session_id);

        // Get initial working directory
        let cwd = sftp
            .canonicalize(".")
            .await
            .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

        Ok(Self {
            sftp,
            session_id,
            cwd,
        })
    }

    /// Get current working directory
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Set current working directory
    pub fn set_cwd(&mut self, path: String) {
        self.cwd = path;
    }

    /// List directory contents
    pub async fn list_dir(
        &self,
        path: &str,
        filter: Option<ListFilter>,
    ) -> Result<Vec<FileInfo>, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        debug!("Listing directory: {}", canonical_path);

        let mut entries = Vec::new();

        // Use read_dir to get directory entries
        let read_dir = self
            .sftp
            .read_dir(&canonical_path)
            .await
            .map_err(|e| self.map_sftp_error(e, &canonical_path))?;

        // Iterate through entries
        for entry in read_dir {
            let name = entry.file_name();

            // Skip . and ..
            if name == "." || name == ".." {
                continue;
            }

            // Apply hidden file filter
            if let Some(ref f) = filter {
                if !f.show_hidden && name.starts_with('.') {
                    continue;
                }
            }

            let full_path = join_remote_path(&canonical_path, &name);

            // Get file metadata
            let metadata = entry.metadata();

            // Determine file type
            let file_type = if metadata.is_dir() {
                FileType::Directory
            } else if metadata.is_symlink() {
                FileType::Symlink
            } else if metadata.is_regular() {
                FileType::File
            } else {
                FileType::Unknown
            };

            // Get symlink target if applicable
            let symlink_target = if file_type == FileType::Symlink {
                self.sftp.read_link(&full_path).await.ok()
            } else {
                None
            };

            // Convert permissions to octal string
            let permissions = metadata
                .permissions
                .map(|p| format!("{:o}", p & 0o777))
                .unwrap_or_else(|| "000".to_string());

            entries.push(FileInfo {
                name,
                path: full_path,
                file_type,
                size: metadata.size.unwrap_or(0),
                modified: metadata.mtime.map(|t| t as i64).unwrap_or(0),
                permissions,
                owner: metadata.uid.map(|u: u32| u.to_string()),
                group: metadata.gid.map(|g: u32| g.to_string()),
                is_symlink: file_type == FileType::Symlink,
                symlink_target,
            });
        }

        // Apply pattern filter
        if let Some(ref f) = filter {
            if let Some(ref pattern) = f.pattern {
                if let Ok(glob_pattern) = glob::Pattern::new(pattern) {
                    entries.retain(|e| glob_pattern.matches(&e.name));
                }
            }
        }

        // Sort entries
        let sort_order = filter.as_ref().map(|f| f.sort).unwrap_or_default();
        self.sort_entries(&mut entries, sort_order);

        debug!("Listed {} entries in {}", entries.len(), canonical_path);
        Ok(entries)
    }

    /// Sort file entries
    fn sort_entries(&self, entries: &mut [FileInfo], order: SortOrder) {
        // Directories always first
        entries.sort_by(|a, b| {
            let a_is_dir = a.file_type == FileType::Directory;
            let b_is_dir = b.file_type == FileType::Directory;

            if a_is_dir != b_is_dir {
                return b_is_dir.cmp(&a_is_dir);
            }

            match order {
                SortOrder::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortOrder::NameDesc => b.name.to_lowercase().cmp(&a.name.to_lowercase()),
                SortOrder::Size => a.size.cmp(&b.size),
                SortOrder::SizeDesc => b.size.cmp(&a.size),
                SortOrder::Modified => a.modified.cmp(&b.modified),
                SortOrder::ModifiedDesc => b.modified.cmp(&a.modified),
                SortOrder::Type => a.name.cmp(&b.name),
                SortOrder::TypeDesc => b.name.cmp(&a.name),
            }
        });
    }

    /// Get file information
    pub async fn stat(&self, path: &str) -> Result<FileInfo, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        debug!("Getting file info: {}", canonical_path);

        let metadata = self
            .sftp
            .metadata(&canonical_path)
            .await
            .map_err(|e| self.map_sftp_error(e, &canonical_path))?;

        let name = Path::new(&canonical_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let file_type = if metadata.is_dir() {
            FileType::Directory
        } else if metadata.is_symlink() {
            FileType::Symlink
        } else if metadata.is_regular() {
            FileType::File
        } else {
            FileType::Unknown
        };

        let symlink_target = if file_type == FileType::Symlink {
            self.sftp.read_link(&canonical_path).await.ok()
        } else {
            None
        };

        let permissions = metadata
            .permissions
            .map(|p| format!("{:o}", p & 0o777))
            .unwrap_or_else(|| "000".to_string());

        Ok(FileInfo {
            name,
            path: canonical_path,
            file_type,
            size: metadata.size.unwrap_or(0),
            modified: metadata.mtime.map(|t| t as i64).unwrap_or(0),
            permissions,
            owner: metadata.uid.map(|u: u32| u.to_string()),
            group: metadata.gid.map(|g: u32| g.to_string()),
            is_symlink: file_type == FileType::Symlink,
            symlink_target,
        })
    }

    /// Write content to a remote file
    ///
    /// This is designed for the IDE mode editor - writes UTF-8 text content
    /// directly to a remote file. The file is created if it doesn't exist,
    /// or truncated and overwritten if it does.
    ///
    /// # Arguments
    /// * `path` - The remote file path to write to
    /// * `content` - The byte content to write (typically UTF-8 text)
    pub async fn write_content(&self, path: &str, content: &[u8]) -> Result<(), SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        debug!(
            "Writing {} bytes to file: {}",
            content.len(),
            canonical_path
        );

        // Open file for writing (create if not exists, truncate if exists)
        let mut file = self
            .sftp
            .open_with_flags(
                &canonical_path,
                OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
            )
            .await
            .map_err(|e| self.map_sftp_error(e, &canonical_path))?;

        // Write the content
        file.write_all(content)
            .await
            .map_err(|e| SftpError::WriteError(format!("Failed to write content: {}", e)))?;

        // Flush and sync to ensure data is written
        file.flush()
            .await
            .map_err(|e| SftpError::WriteError(format!("Failed to flush file: {}", e)))?;

        // File is closed when dropped

        info!("Successfully wrote {} bytes to {}", content.len(), canonical_path);
        Ok(())
    }

    /// Preview file content
    pub async fn preview(&self, path: &str) -> Result<PreviewContent, SftpError> {
        self.preview_with_offset(path, 0).await
    }

    /// Preview file content with offset (for incremental hex loading)
    pub async fn preview_with_offset(
        &self,
        path: &str,
        offset: u64,
    ) -> Result<PreviewContent, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        debug!("Previewing file: {} (offset: {})", canonical_path, offset);

        // Get file info first
        let info = self.stat(&canonical_path).await?;
        let file_size = info.size;

        // Get file name for special handling
        let file_name = Path::new(&canonical_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        // Get file extension
        let extension = Path::new(&canonical_path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Determine MIME type
        let mime_type = mime_guess::from_path(&canonical_path)
            .first_or_octet_stream()
            .to_string();

        // Priority 1: Check by extension first (more reliable for scripts/configs)
        if is_text_extension(&extension) {
            return self
                .preview_text(&canonical_path, &extension, &mime_type)
                .await;
        }
        
        // Priority 1.5: Dotfiles without extension are usually text configs
        // e.g., .gitignore, .env, .htaccess (these have no extension when parsed)
        if file_name.starts_with('.') && extension.is_empty() {
            return self
                .preview_text(&canonical_path, "conf", &mime_type)
                .await;
        }
        // Priority 2: PDF files
        if is_pdf_extension(&extension) || mime_type == "application/pdf" {
            return self.preview_pdf(&canonical_path, file_size).await;
        }

        // Priority 3: Office documents (requires LibreOffice)
        if is_office_extension(&extension) {
            return self.preview_office(&canonical_path, file_size).await;
        }

        // Priority 4: Images
        if mime_type.starts_with("image/") {
            return self
                .preview_image(&canonical_path, file_size, &mime_type)
                .await;
        }

        // Priority 5: Video files
        if is_video_mime(&mime_type)
            || matches!(
                extension.as_str(),
                "mp4" | "webm" | "ogg" | "mov" | "mkv" | "avi"
            )
        {
            return self
                .preview_video(&canonical_path, file_size, &mime_type)
                .await;
        }

        // Priority 6: Audio files
        if is_audio_mime(&mime_type)
            || matches!(
                extension.as_str(),
                "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a"
            )
        {
            return self
                .preview_audio(&canonical_path, file_size, &mime_type)
                .await;
        }

        // Priority 7: Check MIME type for text
        let is_text_mime = mime_type.starts_with("text/")
            || mime_type == "application/json"
            || mime_type == "application/xml"
            || mime_type == "application/javascript"
            || mime_type == "application/toml"
            || mime_type == "application/yaml";

        if is_text_mime {
            return self
                .preview_text(&canonical_path, &extension, &mime_type)
                .await;
        }

        // Priority 8: For files without extension or unknown MIME, detect by content
        // This handles Linux extensionless text files like "fichier", "README", etc.
        if extension.is_empty() || mime_type == "application/octet-stream" {
            // Only attempt content detection for reasonably sized files
            if file_size <= constants::MAX_TEXT_PREVIEW_SIZE {
                // Read a small sample to check if it's text
                let sample_size = file_size.min(8192) as usize;
                if let Ok(sample) = self.read_sample(&canonical_path, sample_size).await {
                    if is_likely_text_content(&sample) {
                        return self
                            .preview_text(&canonical_path, "txt", "text/plain")
                            .await;
                    }
                }
            }
        }

        // Fallback: Hex preview for binary files
        self.preview_hex(&canonical_path, file_size, offset).await
    }

    /// Read a small sample from the beginning of a file for content detection
    async fn read_sample(&self, path: &str, max_bytes: usize) -> Result<Vec<u8>, SftpError> {
        use tokio::io::AsyncReadExt;
        
        let mut file = self
            .sftp
            .open(path)
            .await
            .map_err(|e| SftpError::ProtocolError(e.to_string()))?;
        
        let mut buffer = vec![0u8; max_bytes];
        let bytes_read = file.read(&mut buffer).await.map_err(SftpError::IoError)?;
        buffer.truncate(bytes_read);
        
        Ok(buffer)
    }

    /// Preview text/code files with syntax highlighting hint
    async fn preview_text(
        &self,
        path: &str,
        extension: &str,
        mime_type: &str,
    ) -> Result<PreviewContent, SftpError> {
        let info = self.stat(path).await?;

        // Check size limit for text
        if info.size > constants::MAX_TEXT_PREVIEW_SIZE {
            return Ok(PreviewContent::TooLarge {
                size: info.size,
                max_size: constants::MAX_TEXT_PREVIEW_SIZE,
                recommend_download: true,
            });
        }

        let content = self
            .sftp
            .read(path)
            .await
            .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

        // Detect encoding using chardetng
        let (text, encoding_name, confidence, has_bom) = detect_and_decode(&content);
        let language = extension_to_language(extension);

        Ok(PreviewContent::Text {
            data: text,
            mime_type: Some(mime_type.to_string()),
            language,
            encoding: encoding_name,
            confidence,
            has_bom,
        })
    }

    /// Preview image files
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

        let content = self
            .sftp
            .read(path)
            .await
            .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

        let data = base64::engine::general_purpose::STANDARD.encode(&content);
        Ok(PreviewContent::Image {
            data,
            mime_type: mime_type.to_string(),
        })
    }

    /// Preview video files
    async fn preview_video(
        &self,
        path: &str,
        size: u64,
        mime_type: &str,
    ) -> Result<PreviewContent, SftpError> {
        if size > constants::MAX_MEDIA_PREVIEW_SIZE {
            return Ok(PreviewContent::TooLarge {
                size,
                max_size: constants::MAX_MEDIA_PREVIEW_SIZE,
                recommend_download: true,
            });
        }

        let content = self
            .sftp
            .read(path)
            .await
            .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

        // Correct MIME type for common formats
        let actual_mime = match Path::new(path).extension().and_then(|s| s.to_str()) {
            Some("mp4") => "video/mp4",
            Some("webm") => "video/webm",
            Some("ogg") => "video/ogg",
            Some("mov") => "video/quicktime",
            Some("mkv") => "video/x-matroska",
            Some("avi") => "video/x-msvideo",
            _ => mime_type,
        };

        let data = base64::engine::general_purpose::STANDARD.encode(&content);
        Ok(PreviewContent::Video {
            data,
            mime_type: actual_mime.to_string(),
        })
    }

    /// Preview audio files
    async fn preview_audio(
        &self,
        path: &str,
        size: u64,
        mime_type: &str,
    ) -> Result<PreviewContent, SftpError> {
        if size > constants::MAX_MEDIA_PREVIEW_SIZE {
            return Ok(PreviewContent::TooLarge {
                size,
                max_size: constants::MAX_MEDIA_PREVIEW_SIZE,
                recommend_download: true,
            });
        }

        let content = self
            .sftp
            .read(path)
            .await
            .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

        // Correct MIME type for common formats
        let actual_mime = match Path::new(path).extension().and_then(|s| s.to_str()) {
            Some("mp3") => "audio/mpeg",
            Some("wav") => "audio/wav",
            Some("ogg") => "audio/ogg",
            Some("flac") => "audio/flac",
            Some("aac") => "audio/aac",
            Some("m4a") => "audio/mp4",
            _ => mime_type,
        };

        let data = base64::engine::general_purpose::STANDARD.encode(&content);
        Ok(PreviewContent::Audio {
            data,
            mime_type: actual_mime.to_string(),
        })
    }

    /// Preview PDF files
    async fn preview_pdf(&self, path: &str, size: u64) -> Result<PreviewContent, SftpError> {
        if size > constants::MAX_PREVIEW_SIZE {
            return Ok(PreviewContent::TooLarge {
                size,
                max_size: constants::MAX_PREVIEW_SIZE,
                recommend_download: true,
            });
        }

        let content = self
            .sftp
            .read(path)
            .await
            .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

        let data = base64::engine::general_purpose::STANDARD.encode(&content);
        Ok(PreviewContent::Pdf {
            data,
            original_mime: None,
        })
    }

    /// Preview Office documents (return raw data for frontend rendering)
    async fn preview_office(&self, path: &str, size: u64) -> Result<PreviewContent, SftpError> {
        // Check size limit (increase to 50MB for frontend rendering)
        const MAX_OFFICE_SIZE: u64 = 50 * 1024 * 1024;
        if size > MAX_OFFICE_SIZE {
            return Ok(PreviewContent::TooLarge {
                size,
                max_size: MAX_OFFICE_SIZE,
                recommend_download: true,
            });
        }

        // Get MIME type
        let mime_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        // Download the file
        let content = self
            .sftp
            .read(path)
            .await
            .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

        // Encode to base64 for frontend
        let data = base64::engine::general_purpose::STANDARD.encode(&content);

        Ok(PreviewContent::Office {
            data,
            mime_type,
        })
    }

    /// Preview binary files as hex dump (incremental)
    async fn preview_hex(
        &self,
        path: &str,
        total_size: u64,
        offset: u64,
    ) -> Result<PreviewContent, SftpError> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let chunk_size = constants::HEX_CHUNK_SIZE;

        // Don't read past end of file
        if offset >= total_size {
            return Ok(PreviewContent::Hex {
                data: String::new(),
                total_size,
                offset,
                chunk_size: 0,
                has_more: false,
            });
        }

        // Calculate actual bytes to read
        let bytes_to_read = std::cmp::min(chunk_size, total_size - offset) as usize;

        // Open file and seek to offset
        let mut file = self
            .sftp
            .open(path)
            .await
            .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(SftpError::IoError)?;
        }

        // Read chunk
        let mut buffer = vec![0u8; bytes_to_read];
        let bytes_read = file.read(&mut buffer).await.map_err(SftpError::IoError)?;
        buffer.truncate(bytes_read);

        // Generate hex dump
        let hex_data = generate_hex_dump(&buffer, offset);
        let has_more = offset + (bytes_read as u64) < total_size;

        Ok(PreviewContent::Hex {
            data: hex_data,
            total_size,
            offset,
            chunk_size: bytes_read as u64,
            has_more,
        })
    }


    /// Download directory recursively with progress reporting
    pub async fn download_dir(
        &self,
        remote_path: &str,
        local_path: &str,
        progress_tx: Option<mpsc::Sender<TransferProgress>>,
    ) -> Result<u64, SftpError> {
        let canonical_path = self.resolve_path(remote_path).await?;
        info!("Downloading directory {} to {}", canonical_path, local_path);

        let transfer_id = uuid::Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        // Create local directory
        tokio::fs::create_dir_all(local_path)
            .await
            .map_err(SftpError::IoError)?;

        let total_count = self
            .download_dir_inner(
                &canonical_path,
                local_path,
                &transfer_id,
                &progress_tx,
                &start_time,
            )
            .await?;

        info!("Download directory complete: {} files", total_count);
        Ok(total_count)
    }

    /// Internal recursive directory download implementation
    async fn download_dir_inner(
        &self,
        remote_path: &str,
        local_path: &str,
        transfer_id: &str,
        progress_tx: &Option<mpsc::Sender<TransferProgress>>,
        start_time: &std::time::Instant,
    ) -> Result<u64, SftpError> {
        let entries = self
            .list_dir(
                remote_path,
                Some(ListFilter {
                    show_hidden: true,
                    pattern: None,
                    sort: SortOrder::Name,
                }),
            )
            .await?;

        let mut count = 0u64;

        for entry in entries {
            let local_entry_path = join_local_path(local_path, &entry.name);

            if entry.file_type == FileType::Directory {
                // Create local directory
                tokio::fs::create_dir_all(&local_entry_path)
                    .await
                    .map_err(SftpError::IoError)?;

                // Recurse into subdirectory (boxed to avoid infinite future size)
                count += Box::pin(self.download_dir_inner(
                    &entry.path,
                    &local_entry_path,
                    transfer_id,
                    progress_tx,
                    start_time,
                ))
                .await?;
            } else {
                // Download file
                let content = self
                    .sftp
                    .read(&entry.path)
                    .await
                    .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

                tokio::fs::write(&local_entry_path, &content)
                    .await
                    .map_err(SftpError::IoError)?;

                count += 1;

                // Send progress
                if let Some(ref tx) = progress_tx {
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let speed = if elapsed > 0.0 {
                        (content.len() as f64 / elapsed) as u64
                    } else {
                        0
                    };

                    let _ = tx
                        .send(TransferProgress {
                            id: transfer_id.to_string(),
                            remote_path: entry.path.clone(),
                            local_path: local_entry_path.clone(),
                            direction: TransferDirection::Download,
                            state: TransferState::InProgress,
                            total_bytes: entry.size,
                            transferred_bytes: entry.size,
                            speed,
                            eta_seconds: None,
                            error: None,
                        })
                        .await;
                }
            }
        }

        Ok(count)
    }

    /// Upload directory recursively with progress reporting
    pub async fn upload_dir(
        &self,
        local_path: &str,
        remote_path: &str,
        progress_tx: Option<mpsc::Sender<TransferProgress>>,
    ) -> Result<u64, SftpError> {
        let canonical_path = if is_absolute_remote_path(remote_path) {
            remote_path.to_string()
        } else {
            join_remote_path(&self.cwd, remote_path)
        };
        info!("Uploading directory {} to {}", local_path, canonical_path);

        let transfer_id = uuid::Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        // Create remote directory
        let _ = self.mkdir(&canonical_path).await; // Ignore error if exists

        let total_count = self
            .upload_dir_inner(
                local_path,
                &canonical_path,
                &transfer_id,
                &progress_tx,
                &start_time,
            )
            .await?;

        info!("Upload directory complete: {} files", total_count);
        Ok(total_count)
    }

    /// Internal recursive directory upload implementation
    async fn upload_dir_inner(
        &self,
        local_path: &str,
        remote_path: &str,
        transfer_id: &str,
        progress_tx: &Option<mpsc::Sender<TransferProgress>>,
        start_time: &std::time::Instant,
    ) -> Result<u64, SftpError> {
        let mut entries = tokio::fs::read_dir(local_path)
            .await
            .map_err(SftpError::IoError)?;

        let mut count = 0u64;

        while let Some(entry) = entries.next_entry().await.map_err(SftpError::IoError)? {
            let name = entry.file_name().to_string_lossy().to_string();
            let local_entry_path = entry.path();
            let remote_entry_path = join_remote_path(remote_path, &name);

            let metadata = entry.metadata().await.map_err(SftpError::IoError)?;

            if metadata.is_dir() {
                // Create remote directory
                let _ = self.mkdir(&remote_entry_path).await;

                // Recurse into subdirectory (boxed to avoid infinite future size)
                count += Box::pin(self.upload_dir_inner(
                    local_entry_path.to_string_lossy().as_ref(),
                    &remote_entry_path,
                    transfer_id,
                    progress_tx,
                    start_time,
                ))
                .await?;
            } else {
                // Upload file
                let content = tokio::fs::read(&local_entry_path)
                    .await
                    .map_err(SftpError::IoError)?;

                self.sftp
                    .write(&remote_entry_path, &content)
                    .await
                    .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

                count += 1;

                // Send progress
                if let Some(ref tx) = progress_tx {
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let speed = if elapsed > 0.0 {
                        (content.len() as f64 / elapsed) as u64
                    } else {
                        0
                    };

                    let _ = tx
                        .send(TransferProgress {
                            id: transfer_id.to_string(),
                            remote_path: remote_entry_path.clone(),
                            local_path: local_entry_path.to_string_lossy().to_string(),
                            direction: TransferDirection::Upload,
                            state: TransferState::InProgress,
                            total_bytes: content.len() as u64,
                            transferred_bytes: content.len() as u64,
                            speed,
                            eta_seconds: None,
                            error: None,
                        })
                        .await;
                }
            }
        }

        Ok(count)
    }

    /// Delete file or empty directory
    pub async fn delete(&self, path: &str) -> Result<(), SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        info!("Deleting: {}", canonical_path);

        let info = self.stat(&canonical_path).await?;

        if info.file_type == FileType::Directory {
            self.sftp
                .remove_dir(&canonical_path)
                .await
                .map_err(|e| SftpError::ProtocolError(e.to_string()))?;
        } else {
            self.sftp
                .remove_file(&canonical_path)
                .await
                .map_err(|e| SftpError::ProtocolError(e.to_string()))?;
        }

        Ok(())
    }

    /// Delete file or directory recursively
    pub async fn delete_recursive(&self, path: &str) -> Result<u64, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        info!("Recursively deleting: {}", canonical_path);

        self.delete_recursive_inner(&canonical_path).await
    }

    /// Internal recursive delete implementation
    async fn delete_recursive_inner(&self, path: &str) -> Result<u64, SftpError> {
        let info = self.stat(path).await?;
        let mut deleted_count = 0u64;

        if info.file_type == FileType::Directory {
            // List directory contents
            let entries = self
                .list_dir(
                    path,
                    Some(ListFilter {
                        show_hidden: true,
                        pattern: None,
                        sort: SortOrder::Name,
                    }),
                )
                .await?;

            // Recursively delete each entry (boxed to avoid infinite future size)
            for entry in entries {
                deleted_count += Box::pin(self.delete_recursive_inner(&entry.path)).await?;
            }

            // Delete the now-empty directory
            self.sftp
                .remove_dir(path)
                .await
                .map_err(|e| SftpError::ProtocolError(e.to_string()))?;
            deleted_count += 1;
        } else {
            // Delete file
            self.sftp
                .remove_file(path)
                .await
                .map_err(|e| SftpError::ProtocolError(e.to_string()))?;
            deleted_count += 1;
        }

        Ok(deleted_count)
    }

    /// Create directory
    pub async fn mkdir(&self, path: &str) -> Result<(), SftpError> {
        let canonical_path = if is_absolute_remote_path(path) {
            path.to_string()
        } else {
            join_remote_path(&self.cwd, path)
        };
        info!("Creating directory: {}", canonical_path);

        self.sftp
            .create_dir(&canonical_path)
            .await
            .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

        Ok(())
    }

    /// Rename/move file or directory
    pub async fn rename(&self, old_path: &str, new_path: &str) -> Result<(), SftpError> {
        let old_canonical = self.resolve_path(old_path).await?;
        let new_canonical = if is_absolute_remote_path(new_path) {
            new_path.to_string()
        } else {
            join_remote_path(&self.cwd, new_path)
        };
        info!("Renaming {} to {}", old_canonical, new_canonical);

        self.sftp
            .rename(&old_canonical, &new_canonical)
            .await
            .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

        Ok(())
    }

    /// Download file with resume support
    ///
    /// This method checks for incomplete transfers and resumes from the last position.
    ///
    /// # Arguments
    /// * `remote_path` - Remote file path
    /// * `local_path` - Local file path
    /// * `progress_store` - Progress store for tracking
    /// * `progress_tx` - Optional mpsc sender for UI updates
    /// * `transfer_manager` - Optional transfer manager for control signals
    /// * `transfer_id` - Optional transfer ID (if not provided, generates UUID)
    pub async fn download_with_resume(
        &self,
        remote_path: &str,
        local_path: &str,
        progress_store: std::sync::Arc<dyn ProgressStore>,
        progress_tx: Option<mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<std::sync::Arc<TransferManager>>,
        transfer_id: Option<String>,
    ) -> Result<u64, SftpError> {
        let transfer_id = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let canonical_path = self.resolve_path(remote_path).await?;

        // Register transfer control if manager provided
        let control: Option<std::sync::Arc<super::transfer::TransferControl>> = transfer_manager.as_ref().map(|tm| tm.register(&transfer_id));

        // Check if this is a resume (local file exists)
        let resume_context = if Path::new(local_path).exists() {
            let metadata = tokio::fs::metadata(local_path).await
                .map_err(SftpError::IoError)?;
            let offset = metadata.len();

            info!("Resuming download from offset: {}", offset);

            ResumeContext {
                offset,
                transfer_id: transfer_id.clone(),
                is_resume: true,
            }
        } else {
            ResumeContext {
                offset: 0,
                transfer_id: transfer_id.clone(),
                is_resume: false,
            }
        };

        // Get remote file size
        let info = self.stat(&canonical_path).await?;
        let total_bytes = info.size;

        // ── Smart Butler: Transfer Integrity Check ──
        // If resuming, verify the remote file hasn't changed since the transfer was paused.
        // Compare current remote size with what we previously recorded as total_bytes.
        // Also sanity-check that our offset doesn't exceed the current remote size.
        let resume_context = if resume_context.is_resume {
            // Try to load previously stored progress for this transfer
            let stored = progress_store.load(&resume_context.transfer_id).await.ok().flatten();
            let needs_restart = if let Some(ref sp) = stored {
                if sp.total_bytes != total_bytes {
                    warn!(
                        "Download integrity check: remote file size changed ({} -> {}), restarting from scratch",
                        sp.total_bytes, total_bytes
                    );
                    true
                } else if resume_context.offset > total_bytes {
                    warn!(
                        "Download integrity check: local offset ({}) exceeds remote size ({}), restarting from scratch",
                        resume_context.offset, total_bytes
                    );
                    true
                } else {
                    false
                }
            } else if resume_context.offset > total_bytes {
                warn!(
                    "Download integrity check: local offset ({}) exceeds remote size ({}), restarting from scratch",
                    resume_context.offset, total_bytes
                );
                true
            } else {
                false
            };

            if needs_restart {
                // Delete stale progress record
                if stored.is_some() {
                    let _ = progress_store.delete(&resume_context.transfer_id).await;
                }
                // Truncate the local file to restart
                if let Err(e) = tokio::fs::File::create(local_path).await {
                    warn!("Failed to truncate local file for restart: {}", e);
                }
                ResumeContext {
                    offset: 0,
                    transfer_id: resume_context.transfer_id.clone(),
                    is_resume: false,
                }
            } else {
                resume_context
            }
        } else {
            resume_context
        };

        // Create stored progress
        let mut stored_progress = StoredTransferProgress::new(
            transfer_id.clone(),
            TransferType::Download,
            canonical_path.clone().into(),
            local_path.into(),
            total_bytes,
            self.session_id.clone(),
        );

        if resume_context.is_resume {
            stored_progress.transferred_bytes = resume_context.offset;
        }

        // Execute transfer with retry
        let transferred = transfer_with_retry(
            || self.download_inner(
                &canonical_path,
                local_path,
                &resume_context,
                total_bytes, // Pass total_bytes for progress updates
                progress_tx.clone(),
                control.clone(),
            ),
            RetryConfig::default(),
            progress_store.clone(),
            stored_progress.clone(),
            control.clone(),
        ).await?;

        info!(
            "Download complete: {} ({} bytes)",
            canonical_path, transferred
        );

        Ok(transferred)
    }

    /// Internal download implementation with resume support
    async fn download_inner(
        &self,
        remote_path: &str,
        local_path: &str,
        ctx: &ResumeContext,
        total_bytes: u64, // Total bytes for progress display
        progress_tx: Option<mpsc::Sender<TransferProgress>>,
        control: Option<std::sync::Arc<super::transfer::TransferControl>>,
    ) -> Result<u64, SftpError> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

        /// SFTP I/O timeout to prevent zombie transfers on SSH disconnect (5 minutes)
        const SFTP_IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        // Open remote file
        let mut remote_file = self
            .sftp
            .open(remote_path)
            .await
            .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

        // Seek to offset if resuming
        if ctx.offset > 0 {
            remote_file
                .seek(std::io::SeekFrom::Start(ctx.offset))
                .await
                .map_err(SftpError::IoError)?;

            info!("Seeked remote file to offset: {}", ctx.offset);
        }

        // Open local file (append if resume)
        let mut local_file = if ctx.is_resume {
            tokio::fs::OpenOptions::new()
                .write(true)
                .open(local_path)
                .await
                .map_err(SftpError::IoError)?
        } else {
            tokio::fs::File::create(local_path)
                .await
                .map_err(SftpError::IoError)?
        };

        // Seek local file to end if resume
        if ctx.is_resume {
            local_file
                .seek(std::io::SeekFrom::End(0))
                .await
                .map_err(SftpError::IoError)?;
        }

        // Transfer loop with cooperative cancellation and timeout protection
        let chunk_size = 65536; // 64 KB chunks
        let mut buffer = vec![0u8; chunk_size];
        let mut transferred = ctx.offset;

        loop {
            // Check for cancellation before each read/write cycle
            if let Some(ref ctrl) = control {
                if ctrl.is_cancelled() {
                    info!("Download cancelled during transfer at {} bytes", transferred);
                    return Err(SftpError::TransferCancelled);
                }
                
                // Wait while paused, checking for cancellation
                while ctrl.is_paused() {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    if ctrl.is_cancelled() {
                        info!("Download cancelled while paused at {} bytes", transferred);
                        return Err(SftpError::TransferCancelled);
                    }
                }
            }

            // Read with timeout to prevent zombie transfers on SSH disconnect
            let bytes_read = match tokio::time::timeout(SFTP_IO_TIMEOUT, remote_file.read(&mut buffer)).await {
                Ok(Ok(n)) => n,
                Ok(Err(e)) => return Err(SftpError::ProtocolError(e.to_string())),
                Err(_) => {
                    warn!("SFTP download read timeout after {:?} at {} bytes", SFTP_IO_TIMEOUT, transferred);
                    return Err(SftpError::TransferError(format!(
                        "Read timeout after {:?} - SSH connection may be dead",
                        SFTP_IO_TIMEOUT
                    )));
                }
            };

            if bytes_read == 0 {
                break; // EOF
            }

            // Write to local file (with timeout for consistency)
            match tokio::time::timeout(SFTP_IO_TIMEOUT, local_file.write_all(&buffer[..bytes_read])).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(SftpError::IoError(e)),
                Err(_) => {
                    warn!("SFTP download write timeout after {:?}", SFTP_IO_TIMEOUT);
                    return Err(SftpError::TransferError(format!(
                        "Local write timeout after {:?}",
                        SFTP_IO_TIMEOUT
                    )));
                }
            }

            transferred += bytes_read as u64;

            // Send progress update
            if let Some(ref tx) = progress_tx {
                let _ = tx.send(TransferProgress {
                    id: ctx.transfer_id.clone(),
                    remote_path: remote_path.to_string(),
                    local_path: local_path.to_string(),
                    direction: TransferDirection::Download,
                    state: TransferState::InProgress,
                    total_bytes, // Use actual total_bytes
                    transferred_bytes: transferred,
                    speed: 0,
                    eta_seconds: None,
                    error: None,
                }).await;
            }
        }

        local_file.flush().await.map_err(SftpError::IoError)?;

        Ok(transferred)
    }

    /// Upload file with resume support
    ///
    /// This method uses a .oxide-part temporary file to ensure data integrity.
    ///
    /// # Arguments
    /// * `local_path` - Local file path
    /// * `remote_path` - Remote file path (final destination)
    /// * `progress_store` - Progress store for tracking
    /// * `progress_tx` - Optional mpsc sender for UI updates
    /// * `transfer_manager` - Optional transfer manager for control signals
    /// * `transfer_id` - Optional transfer ID (if not provided, generates UUID)
    ///
    /// # Process
    /// 1. Upload to `remote_path.oxide-part` (protects original file)
    /// 2. If interrupted, resume from last byte in .oxide-part using APPEND mode
    /// 3. Once complete, rename .oxide-part to final filename
    /// 4. If cancelled, clean up .oxide-part file automatically
    pub async fn upload_with_resume(
        &self,
        local_path: &str,
        remote_path: &str,
        progress_store: std::sync::Arc<dyn ProgressStore>,
        progress_tx: Option<mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<std::sync::Arc<TransferManager>>,
        transfer_id: Option<String>,
    ) -> Result<u64, SftpError> {
        let transfer_id = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let canonical_path = self.resolve_path(remote_path).await
            .unwrap_or_else(|_| remote_path.to_string());

        // Use .oxide-part as temporary file
        let temp_path = format!("{}.oxide-part", canonical_path);

        // Register transfer control if manager provided
        let control: Option<std::sync::Arc<super::transfer::TransferControl>> = transfer_manager.as_ref().map(|tm| tm.register(&transfer_id));

        // Get local file size
        let metadata = tokio::fs::metadata(local_path).await
            .map_err(SftpError::IoError)?;
        let total_bytes = metadata.len();

        // ── Smart Butler: Transfer Integrity Check (Upload) ──
        // Before resuming, check if the local source file size matches what was
        // previously stored as total_bytes. If it changed, the source file was
        // modified and we must restart to avoid uploading a corrupt mix.
        let force_restart = {
            // Look up stored progress by listing incomplete transfers and matching paths
            let stored_list = progress_store.list_incomplete(&self.session_id).await.unwrap_or_default();
            let stored = stored_list.iter().find(|sp| {
                sp.transfer_type == super::progress::TransferType::Upload
                    && sp.source_path == PathBuf::from(local_path)
                    && sp.destination_path == PathBuf::from(&canonical_path)
            });
            if let Some(sp) = stored {
                if sp.total_bytes != total_bytes {
                    warn!(
                        "Upload integrity check: local source file size changed ({} -> {}), will restart from scratch",
                        sp.total_bytes, total_bytes
                    );
                    // Delete stale progress
                    let _ = progress_store.delete(&sp.transfer_id).await;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };

        // Check if this is a resume (temp file exists)
        let resume_context = if force_restart {
            // Source file changed — delete remote temp if it exists and start fresh
            if let Ok(_) = self.stat(&temp_path).await {
                info!("Deleting stale remote temp file {} due to source file change", temp_path);
                let _ = self.delete(&temp_path).await;
            }
            ResumeContext {
                offset: 0,
                transfer_id: transfer_id.clone(),
                is_resume: false,
            }
        } else {
            match self.stat(&temp_path).await {
                Ok(remote_info) => {
                    let remote_size = remote_info.size;

                    if remote_size < total_bytes {
                        // Resume from temp file size
                        info!(
                            "Resuming upload from offset: {} (temp file has {} bytes)",
                            remote_size, remote_size
                        );

                        ResumeContext {
                            offset: remote_size,
                            transfer_id: transfer_id.clone(),
                            is_resume: true,
                        }
                    } else {
                        // Temp file already complete, rename to final
                        info!("Temp file already complete ({} bytes), renaming", remote_size);

                        // Rename temp file to final
                        self.rename(&temp_path, &canonical_path).await?;

                        return Ok(total_bytes);
                    }
                }
                Err(_) => {
                    // Temp file doesn't exist, fresh upload
                    ResumeContext {
                        offset: 0,
                        transfer_id: transfer_id.clone(),
                        is_resume: false,
                    }
                }
            }
        };

        // Create stored progress (store final path, not temp path)
        let mut stored_progress = StoredTransferProgress::new(
            transfer_id.clone(),
            TransferType::Upload,
            local_path.into(),
            canonical_path.clone().into(),
            total_bytes,
            self.session_id.clone(),
        );

        if resume_context.is_resume {
            stored_progress.transferred_bytes = resume_context.offset;
        }

        // Execute transfer with retry (upload to temp file)
        let result = transfer_with_retry(
            || self.upload_inner(
                local_path,
                &temp_path, // Upload to temp file
                &resume_context,
                total_bytes,
                progress_tx.clone(),
                control.clone(),
            ),
            RetryConfig::default(),
            progress_store.clone(),
            stored_progress.clone(),
            control.clone(),
        ).await;

        // Handle result
        match result {
            Ok(transferred) => {
                // Final cancellation check before rename (race condition mitigation)
                if let Some(ref ctrl) = control {
                    if ctrl.is_cancelled() {
                        info!("Upload cancelled after completion but before rename, cleaning up {}", temp_path);
                        
                        // Delete temp file
                        if let Err(e) = self.delete(&temp_path).await {
                            warn!("Failed to delete temp file {}: {}", temp_path, e);
                        }
                        
                        // Remove from progress store
                        if let Err(e) = progress_store.delete(&transfer_id).await {
                            warn!("Failed to delete progress for {}: {}", transfer_id, e);
                        }
                        
                        // Unregister from transfer manager
                        if let Some(tm) = transfer_manager {
                            tm.unregister(&transfer_id);
                        }
                        
                        return Err(SftpError::TransferCancelled);
                    }
                }
                
                // Transfer complete, rename temp file to final
                info!(
                    "Upload complete, renaming {} to {}",
                    temp_path, canonical_path
                );

                self.rename(&temp_path, &canonical_path).await?;

                info!(
                    "Upload complete: {} -> {} ({} bytes)",
                    local_path, canonical_path, transferred
                );

                Ok(transferred)
            }
            Err(SftpError::TransferCancelled) => {
                // User cancelled - clean up .oxide-part file
                info!("Upload cancelled, cleaning up {}", temp_path);

                // Delete temp file
                if let Err(e) = self.delete(&temp_path).await {
                    warn!("Failed to delete temp file {}: {}", temp_path, e);
                }

                // Remove from progress store
                if let Err(e) = progress_store.delete(&transfer_id).await {
                    warn!("Failed to delete progress for {}: {}", transfer_id, e);
                }

                // Unregister from transfer manager
                if let Some(tm) = transfer_manager {
                    tm.unregister(&transfer_id);
                }

                Err(SftpError::TransferCancelled)
            }
            Err(e) => {
                // Other error - don't clean up, allow resume
                warn!("Upload failed with error (file preserved for resume): {}", e);
                Err(e)
            }
        }
    }

    /// Internal upload implementation with resume support
    ///
    /// Uses OpenFlags::APPEND for resuming transfers to .oxide-part files
    async fn upload_inner(
        &self,
        local_path: &str,
        remote_path: &str,
        ctx: &ResumeContext,
        total_bytes: u64,
        progress_tx: Option<mpsc::Sender<TransferProgress>>,
        control: Option<std::sync::Arc<super::transfer::TransferControl>>,
    ) -> Result<u64, SftpError> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

        /// SFTP I/O timeout to prevent zombie transfers on SSH disconnect (5 minutes)
        const SFTP_IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

        // Open local file
        let mut local_file = tokio::fs::File::open(local_path).await
            .map_err(SftpError::IoError)?;

        // Seek to offset if resuming
        if ctx.offset > 0 {
            local_file
                .seek(std::io::SeekFrom::Start(ctx.offset))
                .await
                .map_err(SftpError::IoError)?;

            info!("Seeked local file to offset: {}", ctx.offset);
        }

        // Open remote file with appropriate flags
        let mut remote_file = if ctx.is_resume {
            // RESUME: Open existing file with APPEND mode
            // This allows us to continue writing from the end of the file
            info!("Opening remote file with APPEND mode for resume");
            self.sftp
                .open_with_flags(
                    remote_path,
                    OpenFlags::WRITE | OpenFlags::APPEND
                )
                .await
                .map_err(|e| SftpError::ProtocolError(e.to_string()))?
        } else {
            // FRESH UPLOAD: Create new file
            info!("Creating new remote file");
            self.sftp
                .create(remote_path)
                .await
                .map_err(|e| SftpError::ProtocolError(e.to_string()))?
        };

        // Transfer loop with cooperative cancellation and timeout protection
        let chunk_size = 65536; // 64 KB chunks
        let mut buffer = vec![0u8; chunk_size];
        let mut transferred = ctx.offset;

        loop {
            // Check for cancellation before each read/write cycle
            if let Some(ref ctrl) = control {
                if ctrl.is_cancelled() {
                    info!("Upload cancelled during transfer at {} bytes", transferred);
                    return Err(SftpError::TransferCancelled);
                }
                
                // Wait while paused, checking for cancellation
                while ctrl.is_paused() {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    if ctrl.is_cancelled() {
                        info!("Upload cancelled while paused at {} bytes", transferred);
                        return Err(SftpError::TransferCancelled);
                    }
                }
            }

            let bytes_read = local_file
                .read(&mut buffer)
                .await
                .map_err(SftpError::IoError)?;

            if bytes_read == 0 {
                break; // EOF
            }

            // Write to remote file with timeout to prevent zombie transfers
            match tokio::time::timeout(
                SFTP_IO_TIMEOUT,
                AsyncWriteExt::write_all(&mut remote_file, &buffer[..bytes_read])
            ).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(SftpError::ProtocolError(e.to_string())),
                Err(_) => {
                    warn!("SFTP upload write timeout after {:?} at {} bytes", SFTP_IO_TIMEOUT, transferred);
                    return Err(SftpError::TransferError(format!(
                        "Remote write timeout after {:?} - SSH connection may be dead",
                        SFTP_IO_TIMEOUT
                    )));
                }
            }

            transferred += bytes_read as u64;

            // Send progress update
            if let Some(ref tx) = progress_tx {
                let _ = tx.send(TransferProgress {
                    id: ctx.transfer_id.clone(),
                    remote_path: remote_path.to_string(),
                    local_path: local_path.to_string(),
                    direction: TransferDirection::Upload,
                    state: TransferState::InProgress,
                    total_bytes,
                    transferred_bytes: transferred,
                    speed: 0,
                    eta_seconds: None,
                    error: None,
                }).await;
            }
        }

        // Flush remote file (with timeout)
        match tokio::time::timeout(
            SFTP_IO_TIMEOUT,
            AsyncWriteExt::flush(&mut remote_file)
        ).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(SftpError::ProtocolError(e.to_string())),
            Err(_) => {
                warn!("SFTP upload flush timeout after {:?}", SFTP_IO_TIMEOUT);
                return Err(SftpError::TransferError(format!(
                    "Remote flush timeout after {:?} - SSH connection may be dead",
                    SFTP_IO_TIMEOUT
                )));
            }
        }

        info!("Upload inner complete: {} bytes transferred", transferred);

        Ok(transferred)
    }

    /// Resolve relative path to absolute
    async fn resolve_path(&self, path: &str) -> Result<String, SftpError> {
        if is_absolute_remote_path(path) {
            // Already absolute
            self.sftp
                .canonicalize(path)
                .await
                .map_err(|e| SftpError::ProtocolError(e.to_string()))
        } else if path == "~" || path.starts_with("~/") {
            // Home directory
            let home = self
                .sftp
                .canonicalize(".")
                .await
                .map_err(|e| SftpError::ProtocolError(e.to_string()))?;

            if path == "~" {
                Ok(home)
            } else {
                let rest = &path[2..];
                Ok(join_remote_path(&home, rest))
            }
        } else {
            // Relative to cwd
            let full_path = join_remote_path(&self.cwd, path);
            self.sftp
                .canonicalize(&full_path)
                .await
                .map_err(|e| SftpError::ProtocolError(e.to_string()))
        }
    }

    /// Map SFTP errors to our error type
    fn map_sftp_error(&self, err: SftpErrorInner, path: &str) -> SftpError {
        let err_str = err.to_string();
        if err_str.contains("No such file") || err_str.contains("not found") {
            SftpError::FileNotFound(path.to_string())
        } else if err_str.contains("Permission denied") {
            SftpError::PermissionDenied(path.to_string())
        } else {
            SftpError::ProtocolError(err_str)
        }
    }
}

/// Registry of active SFTP sessions
pub struct SftpRegistry {
    sessions: RwLock<HashMap<String, Arc<tokio::sync::Mutex<SftpSession>>>>,
}

impl SftpRegistry {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Register an SFTP session
    pub fn register(&self, session_id: String, session: SftpSession) {
        let mut sessions = self.sessions.write();
        sessions.insert(session_id, Arc::new(tokio::sync::Mutex::new(session)));
    }

    /// Get an SFTP session by ID
    pub fn get(&self, session_id: &str) -> Option<Arc<tokio::sync::Mutex<SftpSession>>> {
        let sessions = self.sessions.read();
        sessions.get(session_id).cloned()
    }

    /// Remove an SFTP session
    pub fn remove(&self, session_id: &str) -> Option<Arc<tokio::sync::Mutex<SftpSession>>> {
        let mut sessions = self.sessions.write();
        sessions.remove(session_id)
    }

    /// Check if a session has SFTP initialized
    pub fn has_sftp(&self, session_id: &str) -> bool {
        let sessions = self.sessions.read();
        sessions.contains_key(session_id)
    }

    /// Close all SFTP sessions (for app shutdown)
    pub async fn close_all(&self) {
        let session_ids: Vec<String> = {
            let sessions = self.sessions.read();
            sessions.keys().cloned().collect()
        };

        tracing::info!("Closing {} SFTP sessions on shutdown", session_ids.len());

        for session_id in session_ids {
            if let Some(session) = self.remove(&session_id) {
                // Lock and drop to ensure cleanup
                let _ = session.lock().await;
            }
        }
    }
}

impl Default for SftpRegistry {
    fn default() -> Self {
        Self::new()
    }
}
