use super::*;
use oxideterm_audit::{AuditCategory, AuditEvidence, AuditOperation, AuditOutcome, AuditSource};
use oxideterm_local_files::{
    calculate_local_checksum, compress_local_files_audited, copy_dropped_local_files,
    create_local_file, create_local_folder, delete_local_entries, duplicate_local_files,
    extract_local_archive_audited, home_path, list_local_files, load_local_bookmarks, local_drives,
    local_file_audit_context, local_file_operation, local_file_properties,
    local_inspection_preview, local_operation_unit_count, local_preview_metadata,
    local_sidebar_locations, open_path_external, paste_local_files, read_local_preview,
    read_local_preview_range, rename_local_entry, reveal_path_external, save_local_bookmarks,
    unique_copy_path,
};

pub(crate) struct DesktopFileSystem;

impl FileSystemBridge for DesktopFileSystem {
    fn home(&self) -> String {
        home_path()
    }

    fn sidebar_locations(&self) -> Vec<LocalSidebarLocation> {
        local_sidebar_locations()
    }

    fn drives(&self) -> Vec<LocalDrive> {
        local_drives()
    }

    fn load_bookmarks(&self, path: &Path) -> FileIo<Result<Vec<LocalBookmark>, String>> {
        FileIo::Ready(Ok(load_local_bookmarks(path)))
    }

    fn save_bookmarks(
        &self,
        path: &Path,
        _: &[LocalBookmark],
        next: &[LocalBookmark],
    ) -> FileIo<Result<(), String>> {
        FileIo::Ready(save_local_bookmarks(path, next))
    }

    fn list(&self, path: &str) -> FileIo<Result<Vec<LocalFileEntry>, String>> {
        FileIo::Ready(list_local_files(path).map_err(|error| error.to_string()))
    }

    fn mutate(&self, dir: &str, action: FileMutation) -> FileIo<Result<(), String>> {
        FileIo::Ready(match action {
            FileMutation::CreateFolder(n) => create_local_folder(dir, &n),
            FileMutation::CreateFile(n) => create_local_file(dir, &n),
            FileMutation::Rename { from, to } => rename_local_entry(dir, &from, &to),
            FileMutation::Delete(names) => delete_local_entries(dir, &names).map_or(Ok(()), Err),
        })
    }

    fn resolve_drop(&self, paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    fn batch_units(&self, paths: &[String]) -> usize {
        paths
            .iter()
            .map(|p| local_operation_unit_count(Path::new(p)))
            .sum()
    }

    fn compress(&self, sources: &[String], destination: &Path) -> FileIo<Result<PathBuf, String>> {
        FileIo::Ready((|| {
            let archive = unique_copy_path(destination);
            compress_local_files_audited(sources, &archive)?;
            Ok(archive)
        })())
    }

    fn extract(&self, archive: &str, destination: &str) -> FileIo<Result<(), String>> {
        FileIo::Ready((|| extract_local_archive_audited(archive, destination))())
    }

    fn properties(&self, entry: &LocalFileEntry) -> LocalFileProperties {
        local_file_properties(entry)
    }

    fn checksum(&self, path: String, cx: &App) -> Task<Result<LocalChecksumResult, String>> {
        cx.background_executor()
            .spawn(async move { calculate_local_checksum(&path) })
    }

    fn metadata(&self, path: &str) -> Option<LocalPreviewMetadata> {
        local_preview_metadata(path)
    }

    fn font_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|error| error.to_string())
    }

    fn image(&self, path: &str, quarter_turns: u8) -> ImageSource {
        ImageSource::Resource(if quarter_turns == 0 {
            gpui::Resource::from(PathBuf::from(path))
        } else {
            gpui::Resource::RotatedImage {
                path: PathBuf::from(path).into(),
                quarter_turns,
            }
        })
    }

    fn open_external(&self, path: &str) -> Result<(), String> {
        open_path_external(path)
    }

    fn reveal(&self, path: &str) -> Result<(), String> {
        reveal_path_external(path)
    }

    fn audio(&self) -> Box<dyn AudioPreviewBackend> {
        Box::new(oxideterm_preview::RodioAudioPreviewBackend::default())
    }

    fn preview(&self, path: &str) -> FileIo<LocalPreview> {
        let audit = local_file_operation("file_preview", path);
        let preview = local_inspection_preview(path, oxideterm_preview::inspection_mime_type)
            .unwrap_or_else(|| read_local_preview(path));
        audit.finish(
            match &preview {
                LocalPreview::Error(_) => AuditOutcome::Failed,
                LocalPreview::TooLarge { .. } | LocalPreview::Unsupported(_) => {
                    AuditOutcome::Unchanged
                }
                LocalPreview::Text { .. }
                | LocalPreview::Markdown { .. }
                | LocalPreview::Archive { .. } => AuditOutcome::Succeeded,
                _ => AuditOutcome::Sent,
            },
            AuditEvidence::Protocol,
            None,
            None,
        );
        FileIo::Ready(preview)
    }

    fn preview_range(
        &self,
        path: &str,
        offset: u64,
        length: u64,
    ) -> FileIo<Result<LocalPreviewChunk, String>> {
        let result = read_local_preview_range(path, offset, length);
        let audit = local_file_operation("file_preview", &format!("path={path}; offset={offset}"));
        audit.finish(
            if result.is_ok() {
                AuditOutcome::Succeeded
            } else {
                AuditOutcome::Failed
            },
            AuditEvidence::Protocol,
            None,
            result.as_ref().ok().map(|chunk| chunk.data.len() as u64),
        );
        FileIo::Ready(result)
    }

    fn pick_folder(&self, prompt: gpui::SharedString, cx: &mut App) -> FolderSelection {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(prompt),
        });
        Box::pin(async move {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return None;
            };
            paths.into_iter().next()
        })
    }

    fn browse_audit(&self) -> BrowseAudit {
        let context = local_file_audit_context(AuditSource::User);
        Box::new(move |path| {
            let audit = AuditOperation::in_context(
                context.as_ref(),
                AuditCategory::File,
                "file_browse",
                Some(path),
            );
            Box::new(move |ok| {
                audit.finish(
                    if ok {
                        AuditOutcome::Succeeded
                    } else {
                        AuditOutcome::Failed
                    },
                    AuditEvidence::Protocol,
                    None,
                    None,
                )
            })
        })
    }

    fn start_batch(
        &self,
        kind: FileBatch,
        paths: Vec<String>,
        total: usize,
        progress: BatchProgress,
        finished: BatchFinished,
        _cx: &mut App,
    ) -> BatchHandle {
        let context = local_file_audit_context(AuditSource::User);
        let worker = std::thread::spawn(move || {
            let result = match kind {
                FileBatch::Paste { destination, mode } => {
                    paste_local_files(paths, destination, mode, total, context, progress)
                }
                FileBatch::Duplicate => duplicate_local_files(paths, total, context, progress),
                FileBatch::Drop { destination } => {
                    copy_dropped_local_files(paths, destination, total, context, progress)
                }
            };
            finished(result);
        });
        Box::new(move || {
            let _ = worker.join();
        })
    }
}
