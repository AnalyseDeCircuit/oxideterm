use js_sys::{Promise, Reflect, Uint8Array};
use oxideterm_gpui_ide::portable::{FileFuture, PortableIdeHost};
use oxideterm_ide_core::*;
use serde::Deserialize;
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::JsFuture;
use web_sys::File;
use zeroize::Zeroizing;

#[wasm_bindgen(module = "/src/device_folder.mjs")]
extern "C" {
    #[wasm_bindgen(js_name = canPickDirectory)]
    pub(crate) fn can_pick_directory() -> bool;
    #[wasm_bindgen(js_name = canMoveEntry)]
    pub(super) fn can_move_entry(directory: bool) -> bool;
    #[wasm_bindgen(js_name = pickDirectory)]
    pub(super) fn pick_directory() -> Promise;
    #[wasm_bindgen(js_name = listDirectory)]
    fn list_directory(root: &JsValue, path: &str) -> Promise;
    #[wasm_bindgen(js_name=openSubdirectory)]
    fn subdir(root: &JsValue, path: &str) -> Promise;
    #[wasm_bindgen(js_name = sameDirectory)]
    fn same_directory(root: &JsValue, other: &JsValue) -> Promise;
    #[wasm_bindgen(js_name = fileSnapshot)]
    fn file_snapshot(root: &JsValue, path: &str) -> Promise;
    #[wasm_bindgen(js_name = saveFile)]
    fn save_file(
        root: &JsValue,
        path: &str,
        bytes: &Uint8Array,
        size: Option<f64>,
        modified: Option<f64>,
        new: bool,
    ) -> Promise;
    #[wasm_bindgen(js_name=startFile)]
    fn start_file(
        root: &JsValue,
        path: &str,
        size: Option<f64>,
        modified: Option<f64>,
        new: bool,
    ) -> Promise;
    #[wasm_bindgen(js_name=writeChunk)]
    fn write_chunk(stream: &JsValue, bytes: &Uint8Array) -> Promise;
    #[wasm_bindgen(js_name=finishFile)]
    fn finish_file(stream: &JsValue) -> Promise;
    #[wasm_bindgen(js_name=abortFile)]
    fn abort_file(stream: &JsValue) -> Promise;

    #[wasm_bindgen(js_name = createDirectory)]
    fn create_directory(root: &JsValue, path: &str) -> Promise;
    #[wasm_bindgen(js_name = removeEntry)]
    fn remove_entry(root: &JsValue, path: &str, recursive: bool) -> Promise;
    #[wasm_bindgen(js_name = moveOrCopy)]
    fn move_or_copy(root: &JsValue, from: &str, to: &str, rename: bool) -> Promise;
}

pub(super) async fn picked(promise: Promise) -> Result<Option<DeviceFolder>, &'static str> {
    let root = JsFuture::from(promise)
        .await
        .map_err(|e| match browser_error(e).kind {
            IdeFileErrorKind::PermissionDenied => "sftp.toast.permission_denied",
            _ => "fileManager.browserFolderFailed",
        })?;
    if root.is_null() {
        return Ok(None);
    }
    let name = Reflect::get(&root, &"name".into())
        .ok()
        .and_then(|v| v.as_string())
        .ok_or("fileManager.browserFolderFailed")?;
    Ok(Some(DeviceFolder { root, name }))
}

#[derive(Clone)]
pub(super) struct DeviceFolder {
    root: JsValue,
    pub(super) name: String,
}

fn browser_error(e: JsValue) -> IdeFileError {
    let name = Reflect::get(&e, &"name".into())
        .ok()
        .and_then(|v| v.as_string());
    IdeFileError::new(
        match name.as_deref() {
            Some("NotAllowedError" | "SecurityError") => IdeFileErrorKind::PermissionDenied,
            Some("NotFoundError") => IdeFileErrorKind::NotFound,
            Some("InvalidStateError" | "NoModificationAllowedError") => IdeFileErrorKind::Conflict,
            Some("NotSupportedError" | "InvalidModificationError") => IdeFileErrorKind::Unsupported,
            _ => IdeFileErrorKind::Other,
        },
        "",
    )
}

async fn result(p: Promise) -> Result<JsValue, IdeFileError> {
    JsFuture::from(p).await.map_err(browser_error)
}

fn file(value: JsValue) -> Result<File, IdeFileError> {
    value
        .dyn_into()
        .map_err(|_| IdeFileError::new(IdeFileErrorKind::Other, ""))
}

pub(super) fn version(file: &File) -> SavedFileVersion {
    SavedFileVersion {
        size_bytes: Some(file.size() as u64),
        modified_millis: Some(file.last_modified() as i64),
        etag: None,
    }
}

#[derive(Deserialize)]
struct Entry {
    name: String,
    directory: bool,
    size: Option<u64>,
    modified: Option<i64>,
}

pub(super) async fn slice(file: &File, start: u64, end: u64) -> Result<Vec<u8>, String> {
    let error = "fileManager.browserImportReadFailed";
    let blob = file
        .slice_with_f64_and_f64(start as f64, end as f64)
        .map_err(|_| error)?;
    let bytes = js_sys::Uint8Array::new(
        &JsFuture::from(blob.array_buffer())
            .await
            .map_err(|_| error)?,
    );
    if u64::from(bytes.length()) != end - start {
        return Err(error.into());
    }
    Ok(bytes.to_vec())
}

pub(super) struct DeviceWriter(Option<JsValue>);

impl DeviceWriter {
    pub(super) async fn write(&self, bytes: &Uint8Array) -> Result<(), IdeFileError> {
        result(write_chunk(self.0.as_ref().unwrap(), bytes))
            .await
            .map(drop)
    }
    pub(super) async fn finish(mut self) -> Result<(), IdeFileError> {
        result(finish_file(self.0.as_ref().unwrap())).await?;
        self.0.take();
        Ok(())
    }
}
impl Drop for DeviceWriter {
    fn drop(&mut self) {
        if let Some(stream) = self.0.take() {
            // Closing a tab must release the writer's exclusive lock, even midway through a transfer.
            wasm_bindgen_futures::spawn_local(async move {
                let _ = JsFuture::from(abort_file(&stream)).await;
            });
        }
    }
}

#[wasm_bindgen(module = "/src/device_bookmarks.mjs")]
extern "C" {
    #[wasm_bindgen(js_name = readFavorites)]
    fn read_favorites(root: &JsValue) -> Promise;
    #[wasm_bindgen(js_name = writeFavorites)]
    fn write_favorites(root: &JsValue, expected: &str, next: &str) -> Promise;
}

impl DeviceFolder {
    pub(super) async fn load_favorites(
        &self,
    ) -> Result<Vec<oxideterm_local_files::LocalBookmark>, String> {
        let invalid = "fileManager.favoritesStorageFailed";
        let raw = JsFuture::from(read_favorites(&self.root))
            .await
            .map_err(|_| invalid)?
            .as_string()
            .ok_or(invalid)?;
        serde_json::from_str(&raw).map_err(|_| invalid.into())
    }

    pub(super) async fn save_favorites(&self, before: String, after: String) -> Result<(), String> {
        match JsFuture::from(write_favorites(&self.root, &before, &after)).await {
            Ok(value) if value.as_bool() == Some(true) => Ok(()),
            Ok(_) => Err("fileManager.favoritesChanged".into()),
            Err(_) => Err("fileManager.favoritesStorageFailed".into()),
        }
    }
    pub(super) async fn writer(
        &self,
        path: &str,
        expected: Option<&SavedFileVersion>,
    ) -> Result<DeviceWriter, IdeFileError> {
        struct Opening(Option<Promise>);
        impl Drop for Opening {
            fn drop(&mut self) {
                if let Some(p) = self.0.take() {
                    wasm_bindgen_futures::spawn_local(async move {
                        if let Ok(stream) = JsFuture::from(p).await {
                            let _ = JsFuture::from(abort_file(&stream)).await;
                        }
                    });
                }
            }
        }
        let promise = start_file(
            &self.root,
            path,
            expected.and_then(|v| v.size_bytes).map(|v| v as f64),
            expected.and_then(|v| v.modified_millis).map(|v| v as f64),
            expected.is_none(),
        );
        let mut opening = Opening(Some(promise.clone()));
        let stream = result(promise).await;
        opening.0.take();
        Ok(DeviceWriter(Some(stream?)))
    }
    pub(super) async fn directory(&self, path: &str) -> Result<Self, IdeFileError> {
        let root = result(subdir(&self.root, path)).await?;
        Ok(Self {
            root,
            name: if path == "/" {
                self.name.clone()
            } else {
                path.rsplit('/').next().unwrap_or(path).into()
            },
        })
    }
    pub(super) async fn same_entry(&self, other: &Self) -> bool {
        JsFuture::from(same_directory(&self.root, &other.root))
            .await
            .ok()
            .and_then(|v| v.as_bool())
            == Some(true)
    }
    pub(super) async fn list(
        &self,
        node: &str,
        path: &str,
    ) -> Result<Vec<FileTreeEntry>, IdeFileError> {
        let data = result(list_directory(&self.root, path)).await?;
        let entries: Vec<Entry> = serde_json::from_str(&data.as_string().unwrap_or_default())
            .map_err(|_| IdeFileError::new(IdeFileErrorKind::Other, ""))?;
        Ok(entries
            .into_iter()
            .map(|e| FileTreeEntry {
                location: IdeLocation::remote(
                    node,
                    format!("{}/{}", path.trim_end_matches('/'), e.name),
                ),
                kind: if e.directory {
                    FileKind::Directory
                } else {
                    FileKind::File
                },
                name: e.name,
                version: SavedFileVersion {
                    size_bytes: e.size,
                    modified_millis: e.modified,
                    etag: None,
                },
            })
            .collect())
    }

    pub(super) async fn snapshot(&self, path: &str) -> Result<File, IdeFileError> {
        file(result(file_snapshot(&self.root, path)).await?)
    }

    pub(super) async fn save_bytes(
        &self,
        path: &str,
        bytes: &[u8],
        expected: Option<&SavedFileVersion>,
        new: bool,
    ) -> Result<SavedFileVersion, IdeFileError> {
        let p = save_file(
            &self.root,
            path,
            &Uint8Array::from(bytes),
            expected.and_then(|v| v.size_bytes).map(|v| v as f64),
            expected.and_then(|v| v.modified_millis).map(|v| v as f64),
            new,
        );
        Ok(version(&file(result(p).await?)?))
    }

    async fn read(&self, path: &str, encoding: Option<&str>) -> Result<IdeFileData, IdeFileError> {
        let f = file(result(file_snapshot(&self.root, path)).await?)?;
        if f.size() > MAX_EDITABLE_FILE_SIZE as f64 {
            return Err(IdeFileError::new(IdeFileErrorKind::TooLarge, ""));
        }
        let bytes = Zeroizing::new(Uint8Array::new(&result(f.array_buffer()).await?).to_vec());
        oxideterm_ide_fs::text::decode_file(&bytes, encoding, version(&f))
    }
}

impl PortableIdeHost for DeviceFolder {
    fn capabilities(&self) -> FileSystemCapabilities {
        FileSystemCapabilities {
            atomic_write: true,
            directory_listing: true,
            conflict_detection: true,
        }
    }
    fn open_project(&self, node: String, path: String) -> FileFuture<'_, IdeProjectInfo> {
        Box::pin(async move {
            let items = self.list(&node, &path).await?;
            Ok(IdeProjectInfo {
                name: if path == "/" {
                    self.name.clone()
                } else {
                    path.rsplit('/').next().unwrap_or(&path).into()
                },
                root_path: path,
                file_count: items.len() as u32,
                is_git_repo: items.iter().any(|e| e.name == ".git"),
                git_branch: None,
            })
        })
    }
    fn list_dir<'a>(&'a self, l: &'a IdeLocation) -> FileFuture<'a, Vec<FileTreeEntry>> {
        Box::pin(async move {
            let IdeLocation::Remote { node_id, path } = l else {
                return Err(IdeFileError::new(IdeFileErrorKind::Unsupported, ""));
            };
            self.list(node_id, path).await
        })
    }
    fn read_file<'a>(
        &'a self,
        l: &'a IdeLocation,
        encoding: Option<&'a str>,
    ) -> FileFuture<'a, IdeFileData> {
        Box::pin(async move { self.read(super::ide::path(l)?, encoding).await })
    }
    fn stat<'a>(&'a self, l: &'a IdeLocation) -> FileFuture<'a, FileStat> {
        Box::pin(async move {
            let f = file(result(file_snapshot(&self.root, super::ide::path(l)?)).await?)?;
            Ok(FileStat {
                version: version(&f),
                is_read_only: false,
            })
        })
    }
    fn write_file<'a>(
        &'a self,
        l: &'a IdeLocation,
        text: &'a SharedText,
        format: &'a TextFileFormat,
        expected: Option<&'a SavedFileVersion>,
        mode: WriteMode,
    ) -> FileFuture<'a, SavedFileVersion> {
        Box::pin(async move {
            let bytes = oxideterm_ide_fs::text::encode_chunks(text.chunks(), format)?;
            let p = save_file(
                &self.root,
                super::ide::path(l)?,
                &Uint8Array::from(bytes.as_slice()),
                expected.and_then(|v| v.size_bytes).map(|n| n as f64),
                expected.and_then(|v| v.modified_millis).map(|n| n as f64),
                mode == WriteMode::CreateNew,
            );
            drop(bytes);
            Ok(version(&file(result(p).await?)?))
        })
    }
    fn create_file(&self, _: String, path: String) -> FileFuture<'_, SavedFileVersion> {
        Box::pin(async move {
            Ok(version(&file(
                result(save_file(
                    &self.root,
                    &path,
                    &Uint8Array::new_with_length(0),
                    None,
                    None,
                    true,
                ))
                .await?,
            )?))
        })
    }
    fn create_folder(&self, _: String, path: String) -> FileFuture<'_, ()> {
        Box::pin(async move { result(create_directory(&self.root, &path)).await.map(drop) })
    }
    fn rename_item(&self, _: String, from: String, to: String) -> FileFuture<'_, ()> {
        Box::pin(async move {
            result(move_or_copy(&self.root, &from, &to, true))
                .await
                .map(drop)
        })
    }
    fn copy_item(&self, _: String, from: String, to: String) -> FileFuture<'_, ()> {
        Box::pin(async move {
            result(move_or_copy(&self.root, &from, &to, false))
                .await
                .map(drop)
        })
    }
    fn delete_item(&self, _: String, path: String, recursive: bool) -> FileFuture<'_, ()> {
        Box::pin(async move {
            result(remove_entry(&self.root, &path, recursive))
                .await
                .map(drop)
        })
    }
    fn search_project(
        &self,
        node: String,
        q: IdeSearchQuery,
    ) -> FileFuture<'_, Vec<IdeSearchMatch>> {
        Box::pin(async move {
            let matcher = q.text_matcher()?;
            let filter = SearchPaths::new(&q)?;
            let mut dirs = vec![q.root_path.clone()];
            let mut hits = Vec::new();
            while let Some(dir) = dirs.pop() {
                for e in self.list(&node, &dir).await? {
                    let IdeLocation::Remote { path, .. } = e.location else {
                        unreachable!()
                    };
                    if e.kind == FileKind::Directory {
                        if (q.include_hidden || !e.name.starts_with('.'))
                            && !matches!(e.name.as_str(), ".git" | "node_modules" | "target")
                        {
                            dirs.push(path);
                        }
                    } else if filter.allows(&path) {
                        let text = match self.read(&path, None).await {
                            Ok(v) => v.text,
                            Err(e)
                                if matches!(
                                    e.kind,
                                    IdeFileErrorKind::Binary
                                        | IdeFileErrorKind::TooLarge
                                        | IdeFileErrorKind::InvalidEncoding
                                        | IdeFileErrorKind::NotFound
                                ) =>
                            {
                                continue;
                            }
                            Err(e) => return Err(e),
                        };
                        for (ix, line) in text.lines().enumerate() {
                            if hits.len() >= q.max_results as usize {
                                return Ok(hits);
                            }
                            if let Some(m) = matcher.find(line) {
                                hits.push(IdeSearchMatch::from_line(
                                    &path,
                                    ix as u32 + 1,
                                    line,
                                    m.range(),
                                ));
                            }
                        }
                    }
                }
            }
            Ok(hits)
        })
    }
}
