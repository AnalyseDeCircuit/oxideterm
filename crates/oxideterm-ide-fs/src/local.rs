// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::text::{check_size, decode_file, encode_file};
use oxideterm_ide_core::{MAX_EDITABLE_FILE_SIZE, TextFileFormat};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
use zeroize::Zeroizing;

use oxideterm_ide_core::{
    AsyncIdeFileSystem, FileKind, FileStat, FileSystemCapabilities, FileTreeEntry, IdeFileCheck,
    IdeFileData, IdeFileError, IdeFileErrorKind, IdeFileSystem, IdeFsFuture, IdeLocation,
    IdePathStat, IdeProjectInfo, SavedFileVersion, WriteMode,
};

#[derive(Clone, Debug, Default)]
pub struct LocalIdeFileSystem;

impl LocalIdeFileSystem {
    pub fn new() -> Self {
        Self
    }

    pub fn open_project(&self, path: impl AsRef<Path>) -> Result<IdeProjectInfo, IdeFileError> {
        let canonical = fs::canonicalize(path.as_ref()).map_err(map_io_error)?;
        let metadata = fs::metadata(&canonical).map_err(map_io_error)?;
        if !metadata.is_dir() {
            return Err(IdeFileError::new(
                IdeFileErrorKind::Other,
                "Path is not a directory",
            ));
        }

        let git_head = canonical.join(".git").join("HEAD");
        let is_git_repo = canonical.join(".git").is_dir();
        let git_branch = if is_git_repo {
            read_git_branch(&git_head)?
        } else {
            None
        };
        let name = canonical
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();

        Ok(IdeProjectInfo {
            root_path: canonical.to_string_lossy().into_owned(),
            name,
            is_git_repo,
            git_branch,
            file_count: 0,
        })
    }

    pub fn check_file(&self, path: impl AsRef<Path>) -> Result<IdeFileCheck, IdeFileError> {
        let metadata = fs::metadata(path.as_ref()).map_err(map_io_error)?;
        if metadata.is_dir() {
            return Ok(IdeFileCheck::NotEditable {
                reason: "Is a directory".to_string(),
            });
        }

        if metadata.len() > MAX_EDITABLE_FILE_SIZE {
            return Ok(IdeFileCheck::TooLarge {
                size: metadata.len(),
                limit: MAX_EDITABLE_FILE_SIZE,
            });
        }

        Ok(IdeFileCheck::Editable {
            size: metadata.len(),
            mtime: metadata_mtime_seconds(&metadata),
        })
    }

    pub fn batch_stat<I, P>(&self, paths: I) -> Vec<Option<IdePathStat>>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        paths
            .into_iter()
            .map(|path| {
                fs::metadata(path.as_ref())
                    .ok()
                    .map(|metadata| IdePathStat {
                        size: metadata.len(),
                        mtime: metadata_mtime_seconds(&metadata),
                        is_dir: metadata.is_dir(),
                    })
            })
            .collect()
    }

    fn local_path<'a>(&self, location: &'a IdeLocation) -> Result<&'a Path, IdeFileError> {
        match location {
            IdeLocation::Local { path } => Ok(path.as_path()),
            IdeLocation::Remote { .. } => Err(IdeFileError::new(
                IdeFileErrorKind::Unsupported,
                "Local IDE filesystem cannot read remote locations",
            )),
        }
    }
}

impl IdeFileSystem for LocalIdeFileSystem {
    fn capabilities(&self) -> FileSystemCapabilities {
        FileSystemCapabilities {
            atomic_write: true,
            directory_listing: true,
            conflict_detection: true,
        }
    }

    fn read_file(
        &self,
        location: &IdeLocation,
        encoding: Option<&str>,
    ) -> Result<IdeFileData, IdeFileError> {
        let audit_context = oxideterm_audit::AuditContext::current_request().map(|mut context| {
            context.target = Some(oxideterm_audit::redact("local"));
            context.protocol = Some("local".into());
            context
        });
        let audit = oxideterm_audit::AuditOperation::in_context(
            audit_context.as_ref(),
            oxideterm_audit::AuditCategory::File,
            "file_open",
            Some(&location.stable_key()),
        );
        let mut read_bytes = None;
        let result = (|| {
            let path = self.local_path(location)?;
            use std::io::Read;
            let file = fs::File::open(path).map_err(map_io_error)?;
            let metadata = file.metadata().map_err(map_io_error)?;
            check_size(metadata.len())?;
            let mut bytes = Zeroizing::new(Vec::new());
            file.take(MAX_EDITABLE_FILE_SIZE + 1)
                .read_to_end(&mut bytes)
                .map_err(map_io_error)?;
            read_bytes = Some(bytes.len() as u64);
            decode_file(&bytes, encoding, version_from_metadata(&metadata))
        })();
        audit.finish(
            if result.is_ok() {
                oxideterm_audit::AuditOutcome::Succeeded
            } else {
                oxideterm_audit::AuditOutcome::Failed
            },
            oxideterm_audit::AuditEvidence::Protocol,
            None,
            read_bytes,
        );
        result
    }

    fn stat(&self, location: &IdeLocation) -> Result<FileStat, IdeFileError> {
        let path = self.local_path(location)?;
        let metadata = fs::metadata(path).map_err(map_io_error)?;
        Ok(FileStat {
            version: version_from_metadata(&metadata),
            is_read_only: metadata.permissions().readonly(),
        })
    }

    fn list_dir(&self, location: &IdeLocation) -> Result<Vec<FileTreeEntry>, IdeFileError> {
        let path = self.local_path(location)?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(path).map_err(map_io_error)? {
            let entry = entry.map_err(map_io_error)?;
            let entry_path = entry.path();
            let metadata = entry.metadata().map_err(map_io_error)?;
            entries.push(FileTreeEntry {
                location: IdeLocation::local(entry_path),
                kind: file_kind_from_metadata(&metadata),
                name: entry.file_name().to_string_lossy().into_owned(),
                version: version_from_metadata(&metadata),
            });
        }
        entries.sort_by(|left, right| {
            file_kind_sort_key(left.kind)
                .cmp(&file_kind_sort_key(right.kind))
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(entries)
    }

    fn write_file(
        &self,
        location: &IdeLocation,
        text: &str,
        format: &TextFileFormat,
        expected_version: Option<&SavedFileVersion>,
        mode: WriteMode,
    ) -> Result<SavedFileVersion, IdeFileError> {
        let audit_context = oxideterm_audit::AuditContext::current_request()
            .or_else(oxideterm_audit::AuditContext::current)
            .map(|mut context| {
                context.target = Some(oxideterm_audit::redact(&location.stable_key()));
                context.protocol = Some("local".into());
                context
            });
        let mut audit = oxideterm_audit::AuditOperation::in_context(
            audit_context.as_ref(),
            oxideterm_audit::AuditCategory::File,
            "file_save",
            Some(&format!(
                "mode={mode:?}; expected_version={}",
                expected_version.is_some()
            )),
        );
        let mut written_bytes = None;
        let audit_result = (|| {
            let bytes = encode_file(text, format)?;
            let path = self.local_path(location)?;
            if mode == WriteMode::CreateNew && path.exists() {
                return Err(IdeFileError::new(
                    IdeFileErrorKind::Conflict,
                    "File already exists",
                ));
            }
            if let Some(expected) = expected_version
                && path.exists()
            {
                let current = version_from_metadata(&fs::metadata(path).map_err(map_io_error)?);
                if local_versions_conflict(expected, &current) {
                    return Err(IdeFileError::new(
                        IdeFileErrorKind::Conflict,
                        "File changed on disk",
                    ));
                }
            }

            match mode {
                WriteMode::AtomicReplace => write_atomic(path, &bytes)?,
                WriteMode::CreateNew => {
                    let mut file = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)
                        .map_err(map_io_error)?;
                    file.write_all(&bytes).map_err(map_io_error)?;
                    file.sync_all().map_err(map_io_error)?;
                }
                WriteMode::CreateOrReplace => fs::write(path, &bytes).map_err(map_io_error)?,
            }

            written_bytes = Some(bytes.len() as u64);
            Ok(version_from_metadata(
                &fs::metadata(path).map_err(map_io_error)?,
            ))
        })();
        audit.summary(&format!(
            "mode={mode:?}; expected_version={}; bytes={}",
            expected_version.is_some(),
            written_bytes.map_or_else(|| "unknown".to_owned(), |bytes| bytes.to_string())
        ));
        audit.finish(
            if audit_result.is_ok() {
                oxideterm_audit::AuditOutcome::Succeeded
            } else if written_bytes.is_some() {
                oxideterm_audit::AuditOutcome::Partial
            } else {
                oxideterm_audit::AuditOutcome::Failed
            },
            oxideterm_audit::AuditEvidence::Protocol,
            None,
            written_bytes,
        );
        audit_result
    }
}

impl AsyncIdeFileSystem for LocalIdeFileSystem {
    fn capabilities(&self) -> FileSystemCapabilities {
        IdeFileSystem::capabilities(self)
    }

    fn read_file<'a>(
        &'a self,
        location: &'a IdeLocation,
        encoding: Option<&'a str>,
    ) -> IdeFsFuture<'a, IdeFileData> {
        Box::pin(async move { IdeFileSystem::read_file(self, location, encoding) })
    }

    fn stat<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, FileStat> {
        Box::pin(async move { IdeFileSystem::stat(self, location) })
    }

    fn list_dir<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, Vec<FileTreeEntry>> {
        Box::pin(async move { IdeFileSystem::list_dir(self, location) })
    }

    fn write_file<'a>(
        &'a self,
        location: &'a IdeLocation,
        text: &'a str,
        format: &'a TextFileFormat,
        expected_version: Option<&'a SavedFileVersion>,
        mode: WriteMode,
    ) -> IdeFsFuture<'a, SavedFileVersion> {
        Box::pin(async move {
            IdeFileSystem::write_file(self, location, text, format, expected_version, mode)
        })
    }
}

fn read_git_branch(head_path: &Path) -> Result<Option<String>, IdeFileError> {
    let Ok(content) = fs::read_to_string(head_path) else {
        return Ok(None);
    };
    if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
        Ok(Some(branch.trim().to_string()))
    } else {
        Ok(Some(content.chars().take(7).collect()))
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), IdeFileError> {
    let swap_path = swap_path(path)?;
    {
        let mut file = fs::File::create(&swap_path).map_err(map_io_error)?;
        file.write_all(bytes).map_err(map_io_error)?;
        file.sync_all().map_err(map_io_error)?;
    }
    fs::rename(&swap_path, path).map_err(map_io_error)
}

fn swap_path(path: &Path) -> Result<PathBuf, IdeFileError> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            IdeFileError::new(IdeFileErrorKind::Other, "Cannot build atomic swap path")
        })?;
    Ok(path.with_file_name(format!(".{file_name}.oxide-ide-swap")))
}

fn local_versions_conflict(expected: &SavedFileVersion, current: &SavedFileVersion) -> bool {
    expected.modified_millis.is_some()
        && current.modified_millis.is_some()
        && expected.modified_millis != current.modified_millis
        || expected.size_bytes.is_some()
            && current.size_bytes.is_some()
            && expected.size_bytes != current.size_bytes
}

fn version_from_metadata(metadata: &fs::Metadata) -> SavedFileVersion {
    SavedFileVersion {
        size_bytes: Some(metadata.len()),
        modified_millis: metadata.modified().ok().and_then(|modified| {
            modified
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_millis() as i64)
        }),
        etag: None,
    }
}

fn metadata_mtime_seconds(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn file_kind_from_metadata(metadata: &fs::Metadata) -> FileKind {
    if metadata.is_dir() {
        FileKind::Directory
    } else if metadata.is_file() {
        FileKind::File
    } else if metadata.file_type().is_symlink() {
        FileKind::Symlink
    } else {
        FileKind::Other
    }
}

fn file_kind_sort_key(kind: FileKind) -> u8 {
    match kind {
        FileKind::Directory => 0,
        FileKind::File => 1,
        FileKind::Symlink => 2,
        FileKind::Other => 3,
    }
}

fn map_io_error(error: io::Error) -> IdeFileError {
    let kind = match error.kind() {
        io::ErrorKind::NotFound => IdeFileErrorKind::NotFound,
        io::ErrorKind::PermissionDenied => IdeFileErrorKind::PermissionDenied,
        io::ErrorKind::TimedOut => IdeFileErrorKind::Timeout,
        io::ErrorKind::ConnectionAborted
        | io::ErrorKind::ConnectionRefused
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::BrokenPipe
        | io::ErrorKind::UnexpectedEof => IdeFileErrorKind::Disconnected,
        io::ErrorKind::AlreadyExists => IdeFileErrorKind::Conflict,
        _ => IdeFileErrorKind::Other,
    };
    IdeFileError::new(kind, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    struct TestKeys;

    impl oxideterm_audit::AuditKeyProvider for TestKeys {
        fn load(&self, _: &str) -> Result<Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
            Ok(Zeroizing::new(vec![13; 32]))
        }

        fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
            self.load(id)
        }
    }

    #[tokio::test]
    async fn explicit_local_open_and_save_record_results_without_background_read() {
        use oxideterm_audit::{
            AuditCategory, AuditContext, AuditOutcome, AuditPhase, AuditQuery, AuditService,
            AuditSource, AuditStore,
        };

        let root = temp_dir();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("note.txt");
        fs::write(&path, b"old").unwrap();
        let location = IdeLocation::local(&path);
        let provider = LocalIdeFileSystem::new();
        let audit_path = root.join("audit.db");
        oxideterm_audit::AuditStore::open(&audit_path, &TestKeys)
            .unwrap()
            .set_policy(oxideterm_audit::AuditPolicy {
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let service = AuditService::with_key_provider(audit_path.clone(), TestKeys).unwrap();
        let context = AuditContext::new(service.client(), AuditSource::User);

        IdeFileSystem::read_file(&provider, &location, None).unwrap();
        context
            .scope(async {
                let data = IdeFileSystem::read_file(&provider, &location, None).unwrap();
                IdeFileSystem::write_file(
                    &provider,
                    &location,
                    "new",
                    &data.format,
                    Some(&data.version),
                    WriteMode::AtomicReplace,
                )
                .unwrap();
            })
            .await;
        drop(service);

        let records = AuditStore::open(&audit_path, &TestKeys)
            .unwrap()
            .query(&AuditQuery {
                category: Some(AuditCategory::File),
                limit: 12,
                ..Default::default()
            })
            .unwrap()
            .records;
        let results = records
            .iter()
            .filter_map(|record| record.details.operation.as_ref())
            .filter(|operation| operation.phase == Some(AuditPhase::Result))
            .map(|operation| {
                (
                    operation.action.as_str(),
                    operation.source,
                    operation.outcome,
                    operation.bytes,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&(
            "file_open",
            AuditSource::User,
            AuditOutcome::Succeeded,
            Some(3)
        )));
        assert!(results.contains(&(
            "file_save",
            AuditSource::User,
            AuditOutcome::Succeeded,
            Some(3)
        )));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn saves_the_original_encoding_and_refuses_unrepresentable_edits() {
        let root = temp_dir();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("legacy.txt");
        let original = b"\xd6\xd0\xce\xc4\r\n";
        fs::write(&path, original).unwrap();
        let provider = LocalIdeFileSystem::new();
        let location = IdeLocation::local(&path);
        let data = IdeFileSystem::read_file(&provider, &location, Some("gb2312")).unwrap();
        assert_eq!(data.text, "中文\n");
        IdeFileSystem::write_file(
            &provider,
            &location,
            &data.text,
            &data.format,
            Some(&data.version),
            WriteMode::AtomicReplace,
        )
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(
            IdeFileSystem::write_file(
                &provider,
                &location,
                "😀",
                &data.format,
                None,
                WriteMode::AtomicReplace
            )
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_adapter_reads_lists_and_writes_atomically() {
        let root = temp_dir();
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("main.rs");
        fs::write(&file_path, "fn main() {}\n").unwrap();
        let fs = LocalIdeFileSystem::new();
        let location = IdeLocation::local(&file_path);

        let data = IdeFileSystem::read_file(&fs, &location, None).unwrap();
        assert_eq!(data.text, "fn main() {}\n");

        let children = IdeFileSystem::list_dir(&fs, &IdeLocation::local(&root)).unwrap();
        assert_eq!(children[0].name, "main.rs");

        let version = IdeFileSystem::write_file(
            &fs,
            &location,
            "fn main() { }\n",
            &data.format,
            Some(&data.version),
            WriteMode::AtomicReplace,
        )
        .unwrap();
        assert_eq!(version.size_bytes, Some(14));
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "fn main() { }\n");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_adapter_detects_conflict() {
        let root = temp_dir();
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("conflict.txt");
        fs::write(&file_path, "old").unwrap();
        let fs = LocalIdeFileSystem::new();
        let location = IdeLocation::local(&file_path);
        let data = IdeFileSystem::read_file(&fs, &location, None).unwrap();
        fs::write(&file_path, "changed").unwrap();

        let error = IdeFileSystem::write_file(
            &fs,
            &location,
            "new",
            &data.format,
            Some(&data.version),
            WriteMode::AtomicReplace,
        )
        .unwrap_err();
        assert_eq!(error.kind, IdeFileErrorKind::Conflict);

        fs::remove_dir_all(root).unwrap();
    }

    fn temp_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("oxideterm-ide-fs-{unique}"))
    }
}
