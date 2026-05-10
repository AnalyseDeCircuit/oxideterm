impl SftpSession {
    pub async fn preview(&self, path: &str) -> Result<PreviewContent, SftpError> {
        self.preview_with_offset(path, 0).await
    }

    pub async fn preview_with_offset(
        &self,
        path: &str,
        offset: u64,
    ) -> Result<PreviewContent, SftpError> {
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
                .preview_asset(&canonical_path, file_size, &mime_type, AssetFileKind::Pdf)
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
}
