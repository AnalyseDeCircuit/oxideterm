use super::memory::MemoryFs;
use crate::demo::{DemoFiles, Entry};
use oxideterm_gpui_ide::portable::*;
use oxideterm_ide_core::*;
use std::path::Path;

fn ready<'a, T: 'a>(v: Result<T, IdeFileError>) -> FileFuture<'a, T> {
    Box::pin(std::future::ready(v))
}
fn error(k: IdeFileErrorKind) -> IdeFileError {
    IdeFileError::new(k, "")
}
fn fs_error(e: String) -> IdeFileError {
    IdeFileError::new(
        match e.as_str() {
            "ENOENT" => IdeFileErrorKind::NotFound,
            "EEXIST" => IdeFileErrorKind::Conflict,
            _ => IdeFileErrorKind::Other,
        },
        e,
    )
}
pub(super) fn path(loc: &IdeLocation) -> Result<&str, IdeFileError> {
    match loc {
        IdeLocation::Remote { path, .. } => Ok(path),
        _ => Err(error(IdeFileErrorKind::Unsupported)),
    }
}
fn version(e: &Entry) -> SavedFileVersion {
    SavedFileVersion {
        size_bytes: e.data.as_ref().map(|v| v.len() as u64),
        etag: Some(e.revision.to_string()),
        modified_millis: e.modified.map(|s|s*1000),
    }
}
impl PortableIdeHost for MemoryFs {
    fn capabilities(&self) -> FileSystemCapabilities {
        FileSystemCapabilities {
            atomic_write: true,
            directory_listing: true,
            conflict_detection: true,
        }
    }
    fn open_project(&self, _: String, p: String) -> FileFuture<'_, IdeProjectInfo> {
        ready(self.files.borrow().list(&p).map_err(fs_error).map(|items| {
            IdeProjectInfo {
                name: Path::new(&p)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                root_path: p,
                file_count: items.len() as u32,
                is_git_repo: false,
                git_branch: None,
            }
        }))
    }
    fn list_dir<'a>(&'a self, l: &'a IdeLocation) -> FileFuture<'a, Vec<FileTreeEntry>> {
        ready((|| {
            let IdeLocation::Remote { node_id, path } = l else {
                return Err(error(IdeFileErrorKind::Unsupported));
            };
            Ok(self
                .files
                .borrow()
                .list(path)
                .map_err(fs_error)?
                .into_iter()
                .map(|(p, e)| FileTreeEntry {
                    location: IdeLocation::remote(node_id, p),
                    is_symlink: false,
                    kind: if e.data.is_some() {
                        FileKind::File
                    } else {
                        FileKind::Directory
                    },
                    name: p.rsplit('/').next().unwrap_or(p).into(),
                    version: version(e),
                })
                .collect())
        })())
    }
    fn read_file<'a>(
        &'a self,
        l: &'a IdeLocation,
        encoding: Option<&'a str>,
    ) -> FileFuture<'a, IdeFileData> {
        ready((|| {
            let fs = self.files.borrow();
            let e = fs
                .entries
                .get(path(l)?)
                .ok_or_else(|| error(IdeFileErrorKind::NotFound))?;
            let bytes = e
                .data
                .as_ref()
                .ok_or_else(|| error(IdeFileErrorKind::Unsupported))?;
            oxideterm_ide_fs::text::decode_file(bytes, encoding, version(e))
        })())
    }
    fn stat<'a>(&'a self, l: &'a IdeLocation) -> FileFuture<'a, FileStat> {
        ready(path(l).and_then(|p| {
            self.files
                .borrow()
                .entries
                .get(p)
                .map(|e| FileStat {
                    version: version(e),
                    is_read_only: false,
                })
                .ok_or_else(|| error(IdeFileErrorKind::NotFound))
        }))
    }
    fn write_file<'a>(
        &'a self,
        l: &'a IdeLocation,
        text: &'a SharedText,
        f: &'a TextFileFormat,
        expected: Option<&'a SavedFileVersion>,
        mode: WriteMode,
    ) -> FileFuture<'a, SavedFileVersion> {
        ready((|| {
            let p = path(l)?;
            let mut fs = self.files.borrow_mut();
            let prev = fs.entries.get(p);
            if matches!(mode, WriteMode::CreateNew) && prev.is_some()
                || expected.is_some_and(|v| prev.map(version).as_ref() != Some(v))
            {
                return Err(error(IdeFileErrorKind::Conflict));
            }
            if prev.is_some_and(|e| e.data.is_none()) {
                return Err(error(IdeFileErrorKind::Unsupported));
            }
            fs.list(Path::new(p).parent().and_then(Path::to_str).unwrap_or("/"))
                .map_err(fs_error)?;
            let mut bytes = oxideterm_ide_fs::text::encode_chunks(text.chunks(), f)?;
            fs.put(p, Some(std::mem::take(&mut *bytes)));
            Ok(version(&fs.entries[p]))
        })())
    }
    fn create_file(&self, _: String, p: String) -> FileFuture<'_, SavedFileVersion> {
        ready(
            self.create(&p, false)
                .map_err(fs_error)
                .map(|_| version(&self.files.borrow().entries[&p])),
        )
    }
    fn create_folder(&self, _: String, p: String) -> FileFuture<'_, ()> {
        ready(self.create(&p, true).map_err(fs_error))
    }
    fn rename_item(&self, _: String, a: String, b: String) -> FileFuture<'_, ()> {
        ready(self.duplicate(&a, &b, true).map_err(fs_error))
    }
    fn copy_item(&self, _: String, a: String, b: String) -> FileFuture<'_, ()> {
        ready(self.duplicate(&a, &b, false).map_err(fs_error))
    }
    fn delete_item(&self, _: String, p: String, r: bool) -> FileFuture<'_, ()> {
        ready(self.remove(&p, r).map_err(fs_error))
    }
    fn search_project(&self, _: String, q: IdeSearchQuery) -> FileFuture<'_, Vec<IdeSearchMatch>> {
        ready((|| {
            let matcher = q.text_matcher()?;
            let paths = SearchPaths::new(&q)?;
            let root = DemoFiles::path("/", &q.root_path);
            let mut hits = Vec::new();
            for (path, entry) in &self.files.borrow().entries {
                let Ok(relative) = Path::new(path).strip_prefix(&root) else {
                    continue;
                };
                if !paths.allows(path)
                    || relative.parent().is_some_and(|parent| {
                        parent.components().any(|c| {
                            matches!(
                                c.as_os_str().to_str(),
                                Some(".git" | "node_modules" | "target")
                            )
                        })
                    })
                {
                    continue;
                }
                let Some(bytes) = &entry.data else { continue };
                let Ok(file) = oxideterm_ide_fs::text::decode_file(bytes, None, version(entry)) else { continue; };
                for (i, line) in file.text.lines().enumerate() {
                    if hits.len() >= q.max_results as usize {
                        return Ok(hits);
                    }
                    if let Some(found) = matcher.find(line) {
                        hits.push(IdeSearchMatch::from_line(
                            path,
                            i as u32 + 1,
                            line,
                            found.range(),
                        ));
                    }
                }
            }
            Ok(hits)
        })())
    }
}
