// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

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

        const MAX_EDITABLE: u64 = 10 * 1024 * 1024;
        if metadata.len() > MAX_EDITABLE {
            return Ok(IdeFileCheck::TooLarge {
                size: metadata.len(),
                limit: MAX_EDITABLE,
            });
        }

        let sample = fs::read(path.as_ref()).map_err(map_io_error)?;
        if sample.contains(&0) || std::str::from_utf8(&sample).is_err() {
            return Ok(IdeFileCheck::Binary);
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

    fn read_file(&self, location: &IdeLocation) -> Result<IdeFileData, IdeFileError> {
        let path = self.local_path(location)?;
        let bytes = fs::read(path).map_err(map_io_error)?;
        let text = String::from_utf8(bytes).map_err(|_| {
            IdeFileError::new(
                IdeFileErrorKind::Unsupported,
                "File is not valid UTF-8 text",
            )
        })?;
        let version = version_from_metadata(&fs::metadata(path).map_err(map_io_error)?);
        Ok(IdeFileData { text, version })
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
        expected_version: Option<&SavedFileVersion>,
        mode: WriteMode,
    ) -> Result<SavedFileVersion, IdeFileError> {
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
            WriteMode::AtomicReplace => write_atomic(path, text.as_bytes())?,
            WriteMode::CreateNew => {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(map_io_error)?;
                file.write_all(text.as_bytes()).map_err(map_io_error)?;
                file.sync_all().map_err(map_io_error)?;
            }
            WriteMode::CreateOrReplace => fs::write(path, text).map_err(map_io_error)?,
        }

        Ok(version_from_metadata(
            &fs::metadata(path).map_err(map_io_error)?,
        ))
    }
}

impl AsyncIdeFileSystem for LocalIdeFileSystem {
    fn capabilities(&self) -> FileSystemCapabilities {
        IdeFileSystem::capabilities(self)
    }

    fn read_file<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, IdeFileData> {
        Box::pin(async move { IdeFileSystem::read_file(self, location) })
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
        expected_version: Option<&'a SavedFileVersion>,
        mode: WriteMode,
    ) -> IdeFsFuture<'a, SavedFileVersion> {
        Box::pin(
            async move { IdeFileSystem::write_file(self, location, text, expected_version, mode) },
        )
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

    #[test]
    fn local_adapter_reads_lists_and_writes_atomically() {
        let root = temp_dir();
        fs::create_dir_all(&root).unwrap();
        let file_path = root.join("main.rs");
        fs::write(&file_path, "fn main() {}\n").unwrap();
        let fs = LocalIdeFileSystem::new();
        let location = IdeLocation::local(&file_path);

        let data = IdeFileSystem::read_file(&fs, &location).unwrap();
        assert_eq!(data.text, "fn main() {}\n");

        let children = IdeFileSystem::list_dir(&fs, &IdeLocation::local(&root)).unwrap();
        assert_eq!(children[0].name, "main.rs");

        let version = IdeFileSystem::write_file(
            &fs,
            &location,
            "fn main() { }\n",
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
        let data = IdeFileSystem::read_file(&fs, &location).unwrap();
        fs::write(&file_path, "changed").unwrap();

        let error = IdeFileSystem::write_file(
            &fs,
            &location,
            "new",
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
