//! Preview data comes from the selected drive, never from a host filesystem path.
use super::memory::MemoryFs;
use gpui::{Image, ImageFormat, ImageSource, Resource};
use oxideterm_ide_core::{LineEnding, TextFileFormat};
use oxideterm_local_files::{LocalPreview, MAX_PREVIEW_SIZE, STREAM_PREVIEW_THRESHOLD};
use oxideterm_preview::{PreviewContent, PreviewKind, TextLineEnding};
use std::sync::Arc;

mod inline;
pub(super) use inline::language;
pub(super) use inline::{format, sftp_content};

pub(super) fn local_preview_bytes(path: &str, bytes: &[u8]) -> LocalPreview {
    let (kind, mime) = format(path, bytes);
    let size = bytes.len() as u64;
    if kind == PreviewKind::Text
        && size > STREAM_PREVIEW_THRESHOLD
        && !mime.eq("text/markdown")
        && !bytes.starts_with(&[0xef, 0xbb, 0xbf])
        && std::str::from_utf8(bytes).is_ok()
    {
        return LocalPreview::TextStream {
            path: path.into(),
            size,
            language: language(path),
        };
    }
    if size > MAX_PREVIEW_SIZE {
        return LocalPreview::TooLarge { size };
    }
    if mime == "application/zip" {
        return match oxideterm_local_files::list_archive_contents(std::io::Cursor::new(bytes)) {
            Ok(info) => LocalPreview::Archive { info },
            Err(_) => LocalPreview::Unsupported("fileManager.archiveInvalid".into()),
        };
    }
    match kind {
        PreviewKind::Image => LocalPreview::Image {
            path: path.into(),
            mime_type: mime,
        },
        PreviewKind::Font => LocalPreview::Font {
            path: path.into(),
            mime_type: mime,
        },
        PreviewKind::Text => {
            let (text, _, _, _, lossy) =
                oxideterm_preview::detect_and_decode_with_hint(bytes, None);
            if lossy {
                return LocalPreview::Unsupported("ide.invalid_encoding".into());
            }
            if language(path).as_deref() == Some("markdown") {
                LocalPreview::Markdown { content: text }
            } else {
                LocalPreview::Text {
                    content: text,
                    language: language(path),
                }
            }
        }
        PreviewKind::Hex => LocalPreview::Unsupported("fileManager.binaryFile".into()),
        _ => LocalPreview::Unsupported("fileManager.unsupportedFormat".into()),
    }
}

impl MemoryFs {
    pub(super) fn file_image(&self, path: &str, turns: u8) -> ImageSource {
        let fs = self.files.borrow();
        let source = fs.entries.get(path).and_then(|entry| {
            let bytes = entry.data.as_deref()?;
            if bytes.len() as u64 > MAX_PREVIEW_SIZE {
                return None;
            }
            let cached = self
                .cached_image
                .borrow()
                .as_ref()
                .and_then(|(rev, image)| {
                    (*rev == entry.revision).then(|| image.upgrade()).flatten()
                });
            let image = if let Some(image) = cached {
                image
            } else {
                let (_, mime) = format(path, bytes);
                let image = Arc::new(Image::from_bytes(
                    ImageFormat::from_mime_type(&mime)?,
                    bytes.to_owned(),
                ));
                self.cached_image
                    .replace(Some((entry.revision, Arc::downgrade(&image))));
                image
            };
            Some(match turns % 4 {
                0 => ImageSource::from(image),
                quarter_turns => ImageSource::Resource(Resource::RotatedBytes {
                    image,
                    quarter_turns,
                }),
            })
        });
        source.unwrap_or_else(|| "lucide/file.svg".into())
    }

    pub(super) fn local_preview(&self, path: &str) -> LocalPreview {
        let fs = self.files.borrow();
        let Some(bytes) = fs.entries.get(path).and_then(|e| e.data.as_deref()) else {
            return LocalPreview::Error("ENOENT".into());
        };
        local_preview_bytes(path, bytes)
    }

    pub(super) fn sftp_preview(
        &self,
        path: &str,
        offset: u64,
    ) -> Result<(u64, PreviewContent), String> {
        let fs = self.files.borrow();
        let entry = fs.entries.get(path).ok_or("ENOENT")?;
        let bytes = entry.data.as_deref().ok_or("EISDIR")?;
        inline::sftp_content(path, bytes, entry.revision, offset).map(|c| (entry.revision, c))
    }

    pub(super) fn save_preview(
        &self,
        path: &str,
        expected: Option<&str>,
        text: &str,
        encoding: &str,
        ending: TextLineEnding,
        bom: bool,
    ) -> Result<oxideterm_sftp::SftpPreviewSaveResult, &'static str> {
        let mut fs = self.files.borrow_mut();
        let expected = expected
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or("ide.conflict_desc")?;
        let format = TextFileFormat {
            encoding: encoding.into(),
            has_bom: bom,
            line_ending: match ending {
                TextLineEnding::Lf => LineEnding::Lf,
                TextLineEnding::CrLf => LineEnding::CrLf,
                TextLineEnding::Cr => LineEnding::Cr,
            },
        };
        let mut data = oxideterm_ide_fs::text::encode_chunks(std::iter::once(text), &format)
            .map_err(|e| match e.kind {
                oxideterm_ide_core::IdeFileErrorKind::UnrepresentableText => {
                    "sftp.preview.unrepresentable"
                }
                oxideterm_ide_core::IdeFileErrorKind::TooLarge => "ide.file_too_large",
                _ => "ide.invalid_encoding",
            })?;
        let size = data.len() as u64;
        let saved = fs
            .replace_file(path, expected, std::mem::take(&mut *data))
            .ok_or("ide.conflict_desc")?;
        Ok(oxideterm_sftp::SftpPreviewSaveResult {
            version: Some(saved.revision.to_string()),
            mtime: saved.modified.and_then(|s| u64::try_from(s).ok()),
            size: Some(size),
            encoding_used: encoding.into(),
            atomic_write: true,
        })
    }
}
