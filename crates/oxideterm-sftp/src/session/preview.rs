impl SftpSession {
    pub async fn preview(&self, path: &str) -> Result<PreviewContent, SftpError> {
        self.preview_with_offset(path, 0).await
    }

    pub async fn preview_with_offset(
        &self,
        path: &str,
        offset: u64,
    ) -> Result<PreviewContent, SftpError> {
        let audit = self.audit_operation("file_preview", path);
        let audit_result = async {
            let canonical_path = self.resolve_path(path).await?;
            let metadata = self
                .sftp
                .metadata(&canonical_path)
                .await
                .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
            let file_size = metadata.size.unwrap_or(0);
            let file_name = Path::new(&canonical_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let extension = Path::new(&canonical_path)
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let mime_type = mime_guess::from_path(&canonical_path)
                .first_or_octet_stream()
                .to_string();
            let header = zeroize::Zeroizing::new(
                self.read_file_limited(&canonical_path, file_size.min(64) as usize)
                    .await?,
            );
            if let Some(mime) = oxideterm_preview::inspection_mime_type(&extension, &header) {
                return self
                    .preview_asset(&canonical_path, file_size, mime, AssetFileKind::Document)
                    .await;
            }

            let sqlite = matches!(extension.as_str(), "sqlite" | "sqlite3")
                || (matches!(extension.as_str(), "db" | "db3")
                    && self.read_file_limited(&canonical_path, 16).await? == b"SQLite format 3\0");
            if sqlite {
                if file_size > constants::MAX_PREVIEW_SIZE {
                    return Ok(PreviewContent::TooLarge {
                        size: file_size,
                        max_size: constants::MAX_PREVIEW_SIZE,
                        recommend_download: true,
                    });
                }
                if !self.sqlite_snapshot_ready(&canonical_path).await? {
                    return Ok(sqlite_snapshot_unavailable());
                }
                let downloaded = self
                    .download_to_temp(&canonical_path, constants::MAX_PREVIEW_SIZE)
                    .await?;
                // Restore RAII ownership during the post-download checks and cancellation boundary.
                let temp =
                    tempfile::TempPath::try_from_path(downloaded).map_err(SftpError::IoError)?;
                let after = self
                    .sftp
                    .metadata(&canonical_path)
                    .await
                    .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
                if !self.sqlite_snapshot_ready(&canonical_path).await?
                    || after.size != metadata.size
                    || after.mtime != metadata.mtime
                {
                    return Ok(sqlite_snapshot_unavailable());
                }
                let path = temp
                    .keep()
                    .map_err(|error| SftpError::IoError(error.error))?;
                return Ok(PreviewContent::AssetFile {
                    path: path.to_string_lossy().into(),
                    mime_type: "application/vnd.sqlite3".into(),
                    kind: AssetFileKind::Document,
                });
            }

            if is_text_extension(&extension) {
                return self
                    .preview_text(&canonical_path, &extension, &mime_type, file_size)
                    .await;
            }
            if file_name.starts_with('.') && extension.is_empty() {
                return self
                    .preview_text(&canonical_path, "conf", &mime_type, file_size)
                    .await;
            }
            if extension == "pdf" || mime_type == "application/pdf" {
                return self
                    .preview_asset(
                        &canonical_path,
                        file_size,
                        &mime_type,
                        AssetFileKind::Document,
                    )
                    .await;
            }
            if is_office_extension(&extension) {
                return self
                    .preview_asset(
                        &canonical_path,
                        file_size,
                        &mime_type,
                        AssetFileKind::Office,
                    )
                    .await;
            }
            if is_font_extension(&extension) || mime_type.starts_with("font/") {
                let font_mime_type = font_mime_type(&extension, &mime_type);
                return self
                    .preview_asset(
                        &canonical_path,
                        file_size,
                        &font_mime_type,
                        AssetFileKind::Font,
                    )
                    .await;
            }
            if mime_type.starts_with("image/") {
                return self
                    .preview_image(&canonical_path, file_size, &mime_type)
                    .await;
            }
            if mime_type.starts_with("video/")
                || matches!(
                    extension.as_str(),
                    "mp4" | "webm" | "ogg" | "mov" | "mkv" | "avi"
                )
            {
                return self
                    .preview_asset(&canonical_path, file_size, &mime_type, AssetFileKind::Video)
                    .await;
            }
            if mime_type.starts_with("audio/")
                || matches!(
                    extension.as_str(),
                    "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a"
                )
            {
                return self
                    .preview_asset(&canonical_path, file_size, &mime_type, AssetFileKind::Audio)
                    .await;
            }

            let is_text_mime = mime_type.starts_with("text/")
                || matches!(
                    mime_type.as_str(),
                    "application/json"
                        | "application/xml"
                        | "application/javascript"
                        | "application/toml"
                        | "application/yaml"
                );
            if is_text_mime {
                return self
                    .preview_text(&canonical_path, &extension, &mime_type, file_size)
                    .await;
            }
            if (extension.is_empty() || mime_type == "application/octet-stream")
                && file_size <= constants::MAX_TEXT_PREVIEW_SIZE
            {
                let sample = self
                    .read_file_limited(&canonical_path, file_size.min(8192) as usize)
                    .await?;
                if is_likely_text_content(&sample) {
                    return self
                        .preview_text(&canonical_path, "txt", "text/plain", file_size)
                        .await;
                }
            }

            self.preview_hex(&canonical_path, file_size, offset).await
        }
        .await;
        audit.result(&audit_result);
        audit_result
    }
}

fn sqlite_snapshot_unavailable() -> PreviewContent {
    PreviewContent::Unsupported {
        mime_type: "application/vnd.sqlite3".into(),
        reason: "file_preview.sqlite_snapshot_required".into(),
    }
}

impl SftpSession {
    async fn sqlite_snapshot_ready(&self, path: &str) -> Result<bool, SftpError> {
        for suffix in ["-wal", "-journal"] {
            let sidecar = format!("{path}{suffix}");
            match self.sftp.metadata(&sidecar).await {
                Ok(metadata) if metadata.size == Some(0) => {}
                Ok(_) => return Ok(false),
                Err(error) => match self.map_sftp_error(error, &sidecar) {
                    SftpError::FileNotFound(_) => {}
                    error => return Err(error),
                },
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    use russh_sftp::{
        protocol::{Attrs, Data, File, Handle, Name, Status, StatusCode},
        server::Handler,
    };

    const PDF: &[u8] = b"%PDF-1.4\ntransport fixture\n%%EOF\n";
    const SQLITE: &[u8] = b"SQLite format 3\0transport fixture";
    struct PreviewServer {
        changed: bool,
    }

    fn fixture(path: &str) -> Option<&'static [u8]> {
        match path {
            "/manual.pdf" => Some(PDF),
            "/bundle.pem" => Some(b"-----BEGIN PRIVATE KEY-----\nfixture"),
            "/executable" => Some(b"\x7fELF\x02\x01"),
            "/snapshot.db" | "/busy.sqlite" | "/changing.sqlite" => Some(SQLITE),
            "/busy.sqlite-wal" => Some(b"pending database writes"),
            _ => None,
        }
    }

    impl Handler for PreviewServer {
        type Error = StatusCode;
        fn unimplemented(&self) -> StatusCode {
            StatusCode::OpUnsupported
        }
        async fn realpath(&mut self, id: u32, path: String) -> Result<Name, StatusCode> {
            Ok(Name {
                id,
                files: vec![File::dummy(path)],
            })
        }
        async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
            let data = fixture(&path).ok_or(StatusCode::NoSuchFile)?;
            Ok(Attrs {
                id,
                attrs: FileAttributes {
                    size: Some(data.len() as u64),
                    mtime: Some(if path == "/changing.sqlite" && self.changed {
                        2
                    } else {
                        1
                    }),
                    permissions: Some(0o100600),
                    ..Default::default()
                },
            })
        }
        async fn open(
            &mut self,
            id: u32,
            path: String,
            _: OpenFlags,
            _: FileAttributes,
        ) -> Result<Handle, StatusCode> {
            Ok(Handle { id, handle: path })
        }
        async fn read(
            &mut self,
            id: u32,
            path: String,
            offset: u64,
            len: u32,
        ) -> Result<Data, StatusCode> {
            let data = fixture(&path).ok_or(StatusCode::NoSuchFile)?;
            if path == "/changing.sqlite" {
                self.changed = true;
            }
            let start = offset as usize;
            if start >= data.len() {
                return Err(StatusCode::Eof);
            }
            Ok(Data {
                id,
                data: bytes::Bytes::copy_from_slice(
                    &data[start..data.len().min(start + len as usize)],
                ),
            })
        }
        async fn close(&mut self, id: u32, _: String) -> Result<Status, StatusCode> {
            Ok(Status {
                id,
                status_code: StatusCode::Ok,
                error_message: String::new(),
                language_tag: String::new(),
            })
        }
    }

    #[tokio::test]
    async fn document_preview_preserves_ownership_and_rejects_incomplete_sqlite_snapshots() {
        let (client, server) = tokio::io::duplex(64 * 1024);
        let server = tokio::spawn(russh_sftp::server::run(
            server,
            PreviewServer { changed: false },
        ));
        let session = SftpSession {
            audit: None,
            sftp: Arc::new(RusshSftpSession::new(client).await.unwrap()),
            channel_factory: Arc::new(|| {
                Box::pin(async { panic!("Preview should reuse its SFTP session") })
            }),
            _connection_owner: None,
            single_channel_transport: true,
            session_id: "preview-test".into(),
            home: "/".into(),
            cwd: "/".into(),
        };
        let content = session.preview("/manual.pdf").await.unwrap();
        let PreviewContent::AssetFile {
            path,
            mime_type,
            kind,
        } = &content
        else {
            panic!("Expected a document asset: {content:?}");
        };
        assert_eq!(*kind, AssetFileKind::Document);
        assert_eq!(mime_type, "application/pdf");
        assert_eq!(std::fs::read(path).unwrap(), PDF);
        let owner =
            oxideterm_preview::PreviewAssetOwner::from_asset_content_owned_temp(&content).unwrap();
        let renderer_lease = owner.clone();
        drop(owner);
        assert!(Path::new(path).exists());
        drop(renderer_lease);
        assert!(!Path::new(path).exists());
        assert_eq!(
            session.sftp.metadata("/manual.pdf").await.unwrap().size,
            Some(PDF.len() as u64)
        );
        let content = session.preview("/snapshot.db").await.unwrap();
        let owner =
            oxideterm_preview::PreviewAssetOwner::from_asset_content_owned_temp(&content).unwrap();
        assert_eq!(owner.mime_type(), "application/vnd.sqlite3");
        assert_eq!(std::fs::read(owner.path()).unwrap(), SQLITE);
        drop(owner);
        for (path, mime) in [
            ("/bundle.pem", "application/pkix-cert"),
            ("/executable", "application/x-oxideterm-binary"),
        ] {
            let content = session.preview(path).await.unwrap();
            assert!(matches!(
                &content,
                PreviewContent::AssetFile {
                    kind: AssetFileKind::Document,
                    ..
                }
            ));
            let owner =
                oxideterm_preview::PreviewAssetOwner::from_asset_content_owned_temp(&content)
                    .unwrap();
            assert_eq!(owner.mime_type(), mime);
            assert_eq!(std::fs::read(owner.path()).unwrap(), fixture(path).unwrap());
            let temporary = owner.path().to_path_buf();
            drop(owner);
            assert!(!temporary.exists());
        }
        for path in ["/busy.sqlite", "/changing.sqlite"] {
            let content = session.preview(path).await.unwrap();
            assert!(
                matches!(content, PreviewContent::Unsupported { reason, .. } if reason == "file_preview.sqlite_snapshot_required")
            );
        }
        drop(session);
        server.abort();
    }
}
