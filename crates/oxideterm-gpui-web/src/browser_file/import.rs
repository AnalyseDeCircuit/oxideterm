use super::{MAX_IMPORT_BYTES, validate_filename};
use gpui::BrowserDropEntry;
use js_sys::{Array, Promise, Uint8Array};
use std::collections::{BTreeMap, VecDeque};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, FileSystemDirectoryEntry, FileSystemDirectoryReader, FileSystemEntry};

const LIMIT: &str = "fileManager.browserImportLimit";
const READ_ERROR: &str = "fileManager.browserImportReadFailed";
const MAX_ENTRIES: usize = 10_000;
const CHUNK: u64 = 1024 * 1024;

#[wasm_bindgen(module = "/src/upload.mjs")]
extern "C" {
    #[wasm_bindgen(js_name=readDirectoryBatch)]
    fn read_batch(reader: &FileSystemDirectoryReader) -> Promise;
    #[wasm_bindgen(js_name=entryFile)]
    fn entry_file(entry: &FileSystemEntry) -> Promise;
}

/// Metadata is collected before prompting. File bytes stay behind the browser handles
/// until the transfer runs, and are moved into the demo drive only after the whole root is read.
pub struct BrowserImport {
    pub name: String,
    pub size: u64,
    pub read: u64,
    pending: VecDeque<(String, File)>,
    tree: BTreeMap<String, Option<Vec<u8>>>,
}

impl BrowserImport {
    pub async fn open(root: BrowserDropEntry) -> Result<Self, String> {
        let name = root.name();
        validate_filename(&name)?;
        let mut import = Self {
            name,
            size: 0,
            read: 0,
            pending: VecDeque::new(),
            tree: BTreeMap::new(),
        };
        match root {
            BrowserDropEntry::File(file) => {
                import.add_file(String::new(), file)?;
            }
            BrowserDropEntry::Directory(dir) => {
                import.tree.insert(String::new(), None);
                let mut dirs = vec![(String::new(), dir)];
                while let Some((parent, dir)) = dirs.pop() {
                    let reader = dir
                        .unchecked_into::<FileSystemDirectoryEntry>()
                        .create_reader();
                    // A nonempty batch is not the end of a directory, even when it has fewer than 100 entries.
                    loop {
                        let value = JsFuture::from(read_batch(&reader))
                            .await
                            .map_err(|_| READ_ERROR)?;
                        let entries = Array::from(&value);
                        if entries.length() == 0 {
                            break;
                        }
                        if import.tree.len() + entries.length() as usize > MAX_ENTRIES {
                            return Err(LIMIT.into());
                        }
                        for value in entries.iter() {
                            let entry: FileSystemEntry = value.unchecked_into();
                            let name = entry.name();
                            validate_filename(&name)?;
                            let path = if parent.is_empty() {
                                name
                            } else {
                                format!("{parent}/{name}")
                            };
                            if import.tree.contains_key(&path) {
                                return Err(READ_ERROR.into());
                            }
                            if entry.is_directory() {
                                import.tree.insert(path.clone(), None);
                                dirs.push((path, entry));
                            } else if entry.is_file() {
                                let file = JsFuture::from(entry_file(&entry))
                                    .await
                                    .map_err(|_| READ_ERROR)?
                                    .dyn_into::<File>()
                                    .map_err(|_| READ_ERROR)?;
                                import.add_file(path, file)?;
                            } else {
                                return Err(READ_ERROR.into());
                            }
                        }
                    }
                }
            }
        }
        Ok(import)
    }

    pub(crate) fn folder(name: String) -> Result<Self, String> {
        validate_filename(&name)?;
        Ok(Self {
            name,
            size: 0,
            read: 0,
            pending: Default::default(),
            tree: BTreeMap::from([(String::new(), None)]),
        })
    }

    pub(crate) fn add_directory(&mut self, path: String) -> Result<(), String> {
        if self.tree.len() >= MAX_ENTRIES {
            return Err(LIMIT.into());
        }
        self.tree.insert(path, None);
        Ok(())
    }

    pub(crate) fn add_file(&mut self, path: String, file: File) -> Result<(), String> {
        if self.tree.len() >= MAX_ENTRIES {
            return Err(LIMIT.into());
        }
        let size = file.size() as u64;
        self.size = self
            .size
            .checked_add(size)
            .filter(|n| *n <= MAX_IMPORT_BYTES)
            .ok_or(LIMIT)?;
        self.tree.insert(path.clone(), Some(Vec::new()));
        if size > 0 {
            self.pending.push_back((path, file));
        }
        Ok(())
    }

    pub fn complete(&self) -> bool {
        self.pending.is_empty()
    }

    pub async fn read_chunk(&mut self) -> Result<(), String> {
        let Some((path, file)) = self.pending.front() else {
            return Ok(());
        };
        let bytes = self
            .tree
            .get_mut(path)
            .and_then(Option::as_mut)
            .expect("pending import is a file");
        let start = bytes.len() as u64;
        let size = file.size() as u64;
        let end = (start + CHUNK).min(size);
        let blob = file
            .slice_with_f64_and_f64(start as f64, end as f64)
            .map_err(|_| READ_ERROR)?;
        let chunk = Uint8Array::new(
            &JsFuture::from(blob.array_buffer())
                .await
                .map_err(|_| READ_ERROR)?,
        );
        if chunk.length() as u64 != end - start {
            return Err(READ_ERROR.into());
        }
        bytes.resize(end as usize, 0);
        chunk.copy_to(&mut bytes[start as usize..]);
        self.read += end - start;
        if end == size {
            self.pending.pop_front();
        }
        Ok(())
    }

    pub fn take_tree(&mut self) -> BTreeMap<String, Option<Vec<u8>>> {
        assert!(self.complete());
        std::mem::take(&mut self.tree)
    }
}
