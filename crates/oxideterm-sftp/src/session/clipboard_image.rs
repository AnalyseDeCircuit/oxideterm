pub const CLIPBOARD_IMAGE_MAX_BYTES: usize = 20 * 1024 * 1024;
pub const CLIPBOARD_IMAGE_DIR_SEGMENTS: [&str; 3] = [".cache", "oxideterm", "clipboard-images"];
const CLIPBOARD_IMAGE_DIR_MODE: u32 = 0o700;
const CLIPBOARD_IMAGE_FILE_MODE: u32 = 0o600;

/// Normalizes a clipboard image extension to a safe ASCII whitelist.
pub fn sanitize_clipboard_image_extension(extension: &str) -> Result<&'static str, SftpError> {
    match extension.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
        "png" => Ok("png"),
        "jpg" | "jpeg" => Ok("jpg"),
        "webp" => Ok("webp"),
        "gif" => Ok("gif"),
        "svg" => Ok("svg"),
        "bmp" => Ok("bmp"),
        "tif" | "tiff" => Ok("tiff"),
        "ico" => Ok("ico"),
        "pnm" => Ok("pnm"),
        other => Err(SftpError::InvalidPath(format!(
            "Unsupported clipboard image extension: {other}"
        ))),
    }
}

/// Builds a unique filename for an uploaded clipboard image.
pub fn format_clipboard_image_filename(extension: &str) -> Result<String, SftpError> {
    let extension = sanitize_clipboard_image_extension(extension)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    Ok(format!("clipboard-{timestamp}-{}.{extension}", &suffix[..8]))
}

/// Formats a remote POSIX path for terminal insertion without quoting safe paths.
pub fn format_posix_shell_path(path: &str) -> String {
    if !path.is_empty()
        && path.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'/' | b':' | b'@' | b'+' | b'-')
        })
    {
        return path.to_string();
    }
    let mut quoted = String::with_capacity(path.len().saturating_add(2));
    quoted.push('\'');
    for character in path.chars() {
        if character == '\'' {
            // Re-enter single quotes after emitting one literal quote outside them.
            quoted.push_str("'\\''");
        } else {
            quoted.push(character);
        }
    }
    quoted.push('\'');
    quoted
}

impl SftpSession {
    /// Uploads a clipboard image into a private user-owned cache directory under
    /// the remote home directory and returns its canonical POSIX file path.
    pub async fn write_private_clipboard_image(
        &self,
        extension: &str,
        bytes: &[u8],
    ) -> Result<String, SftpError> {
        if bytes.is_empty() {
            return Err(SftpError::WriteError(
                "Clipboard image is empty".to_string(),
            ));
        }
        if bytes.len() > CLIPBOARD_IMAGE_MAX_BYTES {
            return Err(SftpError::WriteError(format!(
                "Clipboard image exceeds the {} MiB limit",
                CLIPBOARD_IMAGE_MAX_BYTES / (1024 * 1024)
            )));
        }

        let filename = format_clipboard_image_filename(extension)?;
        let home = if !self.home.is_empty() && is_absolute_remote_path(&self.home) {
            self.home.clone()
        } else {
            self.sftp
                .canonicalize(".")
                .await
                .map_err(|error| self.map_sftp_error(error, "."))?
        };
        let target_dir = CLIPBOARD_IMAGE_DIR_SEGMENTS
            .iter()
            .fold(home.clone(), |current, segment| {
                join_remote_path(&current, segment)
            });
        let remote_path = join_remote_path(&target_dir, &filename);
        let audit = self.audit_operation(
            "file_clipboard_image_upload",
            &format!("{remote_path} ({} bytes)", bytes.len()),
        );
        let audit_result = async {
            let mut current_dir = home;
            for (index, segment) in CLIPBOARD_IMAGE_DIR_SEGMENTS.iter().enumerate() {
                current_dir = join_remote_path(&current_dir, segment);
                // Enforce 0700 on OxideTerm-owned cache directories, leaving ~/.cache's
                // existing mode intact when it already exists.
                let enforce_private_mode = index >= 1;
                self.ensure_private_remote_directory(&current_dir, enforce_private_mode)
                    .await?;
            }

            let mut file = self
                .sftp
                .open_with_flags(
                    &remote_path,
                    OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
                )
                .await
                .map_err(|error| self.map_sftp_error(error, &remote_path))?;

            let write_result: Result<(), SftpError> = async {
                file.write_all(bytes).await.map_err(|error| {
                    SftpError::WriteError(format!("Failed to write clipboard image: {error}"))
                })?;
                file.flush().await.map_err(|error| {
                    SftpError::WriteError(format!("Failed to flush clipboard image: {error}"))
                })?;
                file.shutdown().await.map_err(SftpError::IoError)?;
                Ok(())
            }
            .await;

            if let Err(error) = write_result {
                let _ = self.sftp.remove_file(&remote_path).await;
                return Err(error);
            }

            self.apply_private_permissions(&remote_path, CLIPBOARD_IMAGE_FILE_MODE)
                .await?;

            Ok(remote_path.clone())
        }
        .await;
        audit.result(&audit_result);
        audit_result
    }

    async fn ensure_private_remote_directory(
        &self,
        path: &str,
        enforce_private_mode: bool,
    ) -> Result<(), SftpError> {
        let mut created = false;
        match self.sftp.symlink_metadata(path).await {
            Ok(metadata) => {
                if metadata.is_symlink() {
                    return Err(SftpError::PermissionDenied(format!(
                        "Refusing to follow symbolic link for clipboard image directory: {path}"
                    )));
                }
                if !metadata.is_dir() {
                    return Err(SftpError::InvalidPath(format!(
                        "Clipboard image directory path is not a directory: {path}"
                    )));
                }
            }
            Err(SftpErrorInner::Status(status))
                if status.status_code == russh_sftp::protocol::StatusCode::NoSuchFile =>
            {
                match self.sftp.create_dir(path).await {
                    Ok(()) => {
                        created = true;
                    }
                    Err(create_error) => {
                        // Tolerate a concurrent mkdir by verifying the path is a real directory.
                        match self.sftp.symlink_metadata(path).await {
                            Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {}
                            Ok(metadata) if metadata.is_symlink() => {
                                return Err(SftpError::PermissionDenied(format!(
                                    "Refusing to follow symbolic link for clipboard image directory: {path}"
                                )));
                            }
                            _ => return Err(self.map_sftp_error(create_error, path)),
                        }
                    }
                }
            }
            Err(error) => return Err(self.map_sftp_error(error, path)),
        }

        if enforce_private_mode || created {
            self.apply_private_permissions(path, CLIPBOARD_IMAGE_DIR_MODE)
                .await?;
        }
        Ok(())
    }

    async fn apply_private_permissions(&self, path: &str, mode: u32) -> Result<(), SftpError> {
        let attrs = FileAttributes {
            permissions: Some(mode),
            ..FileAttributes::empty()
        };
        if let Err(error) = self.sftp.set_metadata(path, attrs).await {
            // Some SFTP v3 servers (or Windows OpenSSH) do not support SETSTAT.
            if !matches!(
                &error,
                SftpErrorInner::Status(status)
                    if status.status_code == russh_sftp::protocol::StatusCode::OpUnsupported
            ) {
                return Err(self.map_sftp_error(error, path));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod clipboard_image_tests {
    use super::*;
    use russh_sftp::{
        protocol::{Attrs, File, Handle, Name, Status, StatusCode, Version},
        server::Handler,
    };
    use std::collections::HashSet;
    use std::sync::Mutex;

    #[derive(Default)]
    struct ClipboardServerState {
        dirs: HashSet<String>,
        symlinks: HashSet<String>,
        files: HashMap<String, Vec<u8>>,
        modes: HashMap<String, u32>,
        reject_setstat: bool,
    }

    struct ClipboardServer {
        home: String,
        state: Arc<Mutex<ClipboardServerState>>,
    }

    fn ok_status(id: u32) -> Status {
        Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        }
    }

    impl Handler for ClipboardServer {
        type Error = StatusCode;

        fn unimplemented(&self) -> StatusCode {
            StatusCode::OpUnsupported
        }

        async fn init(
            &mut self,
            _: u32,
            _: HashMap<String, String>,
        ) -> Result<Version, StatusCode> {
            Ok(Version::new())
        }

        async fn realpath(&mut self, id: u32, path: String) -> Result<Name, StatusCode> {
            let resolved = if path == "." {
                self.home.clone()
            } else {
                path
            };
            Ok(Name {
                id,
                files: vec![File::dummy(resolved)],
            })
        }

        async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
            let state = self.state.lock().unwrap();
            if state.symlinks.contains(&path) {
                return Ok(Attrs {
                    id,
                    attrs: FileAttributes {
                        permissions: Some(0o120777),
                        ..Default::default()
                    },
                });
            }
            if state.dirs.contains(&path) {
                let mode = state.modes.get(&path).copied().unwrap_or(0o755);
                return Ok(Attrs {
                    id,
                    attrs: FileAttributes {
                        permissions: Some(0o040000 | mode),
                        ..Default::default()
                    },
                });
            }
            if let Some(bytes) = state.files.get(&path) {
                let mode = state.modes.get(&path).copied().unwrap_or(0o644);
                return Ok(Attrs {
                    id,
                    attrs: FileAttributes {
                        size: Some(bytes.len() as u64),
                        permissions: Some(0o100000 | mode),
                        ..Default::default()
                    },
                });
            }
            Err(StatusCode::NoSuchFile)
        }

        async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
            self.lstat(id, path).await
        }

        async fn mkdir(
            &mut self,
            id: u32,
            path: String,
            _: FileAttributes,
        ) -> Result<Status, StatusCode> {
            let mut state = self.state.lock().unwrap();
            if state.dirs.contains(&path) || state.files.contains_key(&path) {
                return Err(StatusCode::Failure);
            }
            state.dirs.insert(path);
            Ok(ok_status(id))
        }

        async fn setstat(
            &mut self,
            id: u32,
            path: String,
            attrs: FileAttributes,
        ) -> Result<Status, StatusCode> {
            let mut state = self.state.lock().unwrap();
            if state.reject_setstat {
                return Err(StatusCode::OpUnsupported);
            }
            if let Some(permissions) = attrs.permissions {
                state.modes.insert(path, permissions & 0o777);
            }
            Ok(ok_status(id))
        }

        async fn open(
            &mut self,
            id: u32,
            path: String,
            flags: OpenFlags,
            _: FileAttributes,
        ) -> Result<Handle, StatusCode> {
            let mut state = self.state.lock().unwrap();
            if flags.contains(OpenFlags::CREATE) {
                if flags.contains(OpenFlags::EXCLUDE) && state.files.contains_key(&path) {
                    return Err(StatusCode::Failure);
                }
                state.files.entry(path.clone()).or_default();
            }
            if !state.files.contains_key(&path) {
                return Err(StatusCode::NoSuchFile);
            }
            Ok(Handle { id, handle: path })
        }

        async fn write(
            &mut self,
            id: u32,
            handle: String,
            offset: u64,
            data: Vec<u8>,
        ) -> Result<Status, StatusCode> {
            let mut state = self.state.lock().unwrap();
            let file = state.files.get_mut(&handle).ok_or(StatusCode::NoSuchFile)?;
            let start = offset as usize;
            let end = start + data.len();
            if file.len() < end {
                file.resize(end, 0);
            }
            file[start..end].copy_from_slice(&data);
            Ok(ok_status(id))
        }

        async fn close(&mut self, id: u32, _: String) -> Result<Status, StatusCode> {
            Ok(ok_status(id))
        }
    }

    async fn clipboard_session(
        home: &str,
        state: ClipboardServerState,
    ) -> (SftpSession, Arc<Mutex<ClipboardServerState>>) {
        let state = Arc::new(Mutex::new(state));
        let (client, server) = tokio::io::duplex(128 * 1024);
        tokio::spawn(russh_sftp::server::run(
            server,
            ClipboardServer {
                home: home.to_string(),
                state: state.clone(),
            },
        ));
        let session = SftpSession {
            audit: None,
            sftp: Arc::new(RusshSftpSession::new(client).await.unwrap()),
            channel_factory: Arc::new(|| {
                Box::pin(async { panic!("test must use its owned SFTP channel") })
            }),
            _connection_owner: None,
            single_channel_transport: true,
            session_id: "clipboard-image-test".into(),
            home: home.to_string(),
            cwd: home.to_string(),
        };
        (session, state)
    }

    #[test]
    fn format_posix_shell_path_leaves_safe_paths_unquoted_and_escapes_special_chars() {
        assert_eq!(
            format_posix_shell_path(
                "/home/alice/.cache/oxideterm/clipboard-images/clipboard-20261127-130000-deadbeef.png"
            ),
            "/home/alice/.cache/oxideterm/clipboard-images/clipboard-20261127-130000-deadbeef.png"
        );
        assert_eq!(
            format_posix_shell_path("/home/alice bob/.cache/oxideterm/clipboard-images/a'b.png"),
            "'/home/alice bob/.cache/oxideterm/clipboard-images/a'\\''b.png'"
        );
    }

    #[test]
    fn sanitize_clipboard_image_extension_rejects_traversal_and_unknown_types() {
        assert_eq!(sanitize_clipboard_image_extension("PNG").unwrap(), "png");
        assert_eq!(sanitize_clipboard_image_extension(".jpeg").unwrap(), "jpg");
        assert!(sanitize_clipboard_image_extension("../sh").is_err());
        assert!(sanitize_clipboard_image_extension("exe").is_err());
    }

    #[tokio::test]
    async fn write_private_clipboard_image_creates_private_dirs_and_writes_payload() {
        let mut initial = ClipboardServerState::default();
        initial.dirs.insert("/home/alice".to_string());
        let (session, state) = clipboard_session("/home/alice", initial).await;
        let payload = b"\x89PNG\r\n\x1a\nfake-png-bytes";

        let path = session
            .write_private_clipboard_image("png", payload)
            .await
            .expect("upload should succeed");

        assert!(
            path.starts_with("/home/alice/.cache/oxideterm/clipboard-images/clipboard-"),
            "unexpected path: {path}"
        );
        assert!(path.ends_with(".png"), "unexpected extension: {path}");

        let state = state.lock().unwrap();
        assert_eq!(state.files.get(&path).map(Vec::as_slice), Some(payload.as_slice()));
        assert_eq!(state.modes.get("/home/alice/.cache/oxideterm"), Some(&0o700));
        assert_eq!(
            state
                .modes
                .get("/home/alice/.cache/oxideterm/clipboard-images"),
            Some(&0o700)
        );
        assert_eq!(state.modes.get(&path), Some(&0o600));
    }

    #[tokio::test]
    async fn write_private_clipboard_image_rejects_symlink_directory_and_oversized_payload() {
        let mut initial = ClipboardServerState::default();
        initial.dirs.insert("/home/alice".to_string());
        initial.dirs.insert("/home/alice/.cache".to_string());
        initial
            .symlinks
            .insert("/home/alice/.cache/oxideterm".to_string());
        let (session, state) = clipboard_session("/home/alice", initial).await;

        let err = session
            .write_private_clipboard_image("png", b"png-bytes")
            .await
            .expect_err("symlink directory must be rejected");
        assert!(matches!(err, SftpError::PermissionDenied(_)));
        assert!(state.lock().unwrap().files.is_empty());

        let oversized = vec![0u8; CLIPBOARD_IMAGE_MAX_BYTES + 1];
        let size_err = session
            .write_private_clipboard_image("png", &oversized)
            .await
            .expect_err("oversized payload must be rejected");
        assert!(matches!(size_err, SftpError::WriteError(_)));
    }

    #[tokio::test]
    async fn write_private_clipboard_image_succeeds_when_setstat_is_unsupported() {
        let mut initial = ClipboardServerState {
            reject_setstat: true,
            ..Default::default()
        };
        initial.dirs.insert("/home/alice".to_string());
        let (session, state) = clipboard_session("/home/alice", initial).await;

        let path = session
            .write_private_clipboard_image("webp", b"RIFFwebp")
            .await
            .expect("upload should tolerate OpUnsupported SETSTAT");
        assert!(path.ends_with(".webp"));
        assert_eq!(
            state.lock().unwrap().files.get(&path).map(Vec::as_slice),
            Some(b"RIFFwebp".as_slice())
        );
    }
}
