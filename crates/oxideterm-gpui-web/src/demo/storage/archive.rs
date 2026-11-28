use super::DemoFiles;
use chrono::{Datelike, Timelike};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

// The archive and its extracted files share the browser's WASM heap.
pub(crate) const MAX_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_ENTRIES: usize = 10_000;
const LIMIT: &str = "fileManager.archiveLimit";
const INVALID: &str = "fileManager.archiveInvalid";
const CONFLICT: &str = "sftp.conflict.description";

impl DemoFiles {
    pub fn compress_zip(
        &mut self,
        sources: &[String],
        destination: &Path,
    ) -> Result<PathBuf, String> {
        let parent = destination.parent().and_then(Path::to_str).ok_or(INVALID)?;
        self.list(parent)?;
        let output = oxideterm_local_files::unique_copy_path_with(destination, |path| {
            path.to_str().is_some_and(|p| self.entries.contains_key(p))
        });
        let bytes = self.zip_selection(sources)?;
        self.put(output.to_str().ok_or(INVALID)?, Some(bytes));
        Ok(output)
    }

    pub fn zip_selection(&self, sources: &[String]) -> Result<Vec<u8>, String> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut count = 0;
        let mut total = 0u64;
        for source in sources {
            if !self.entries.contains_key(source) {
                return Err("ENOENT".into());
            }
            let root = Path::new(source);
            let parent = root.parent().ok_or(INVALID)?;
            for (path, entry) in self.entries.range(source.clone()..) {
                if !path.starts_with(source) {
                    break;
                }
                let path = Path::new(path);
                if !path.starts_with(root) {
                    continue;
                }
                let name = path
                    .strip_prefix(parent)
                    .ok()
                    .and_then(Path::to_str)
                    .ok_or(INVALID)?;
                count += 1;
                total += entry.data.as_ref().map_or(0, |v| v.len() as u64);
                if count > MAX_ENTRIES || total > MAX_BYTES {
                    return Err(LIMIT.into());
                }
                let options = entry
                    .modified
                    .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
                    .and_then(|t| {
                        zip::DateTime::from_date_and_time(
                            t.year().try_into().ok()?,
                            t.month() as u8,
                            t.day() as u8,
                            t.hour() as u8,
                            t.minute() as u8,
                            t.second() as u8,
                        )
                        .ok()
                    })
                    .map_or(options, |date| options.last_modified_time(date));
                match &entry.data {
                    Some(data) => {
                        zip.start_file(name, options).map_err(|_| INVALID)?;
                        zip.write_all(data).map_err(|_| INVALID)?;
                    }
                    None => zip.add_directory(name, options).map_err(|_| INVALID)?,
                }
            }
        }
        let bytes = zip.finish().map_err(|_| INVALID)?.into_inner();
        if bytes.len() as u64 > MAX_BYTES {
            return Err(LIMIT.into());
        }
        Ok(bytes)
    }

    pub fn extract_zip(&mut self, archive_path: &str, destination: &str) -> Result<(), String> {
        self.list(destination)?;
        let bytes = self
            .entries
            .get(archive_path)
            .and_then(|e| e.data.as_deref())
            .ok_or("ENOENT")?;
        let pending = unpack_zip(bytes, destination, |p| {
            self.entries.get(p).map(|e| e.data.is_some())
        })?;
        // Publish only after every member and destination has passed validation.
        for (path, bytes) in pending {
            self.put(&path, bytes);
        }
        Ok(())
    }
}

pub(crate) fn unpack_zip(
    bytes: &[u8],
    destination: &str,
    existing: impl Fn(&str) -> Option<bool>,
) -> Result<BTreeMap<String, Option<Vec<u8>>>, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(LIMIT.into());
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| INVALID)?;
    if archive.len() > MAX_ENTRIES {
        return Err(LIMIT.into());
    }
    let root = Path::new(destination);
    let mut pending: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
    let mut remaining = MAX_BYTES;
    for i in 0..archive.len() {
        let member = archive.by_index(i).map_err(|_| INVALID)?;
        if member.name().len() > 4096 {
            return Err(INVALID.into());
        }
        let name = member.enclosed_name().ok_or(INVALID)?;
        if member.is_symlink()
            || member.name().contains(['\\', '\0'])
            || name
                .components()
                .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(INVALID.into());
        }
        let path = root.join(
            name.components()
                .filter(|c| *c != Component::CurDir)
                .collect::<PathBuf>(),
        );
        if path == root && !member.is_dir() {
            return Err(INVALID.into());
        }
        let is_dir = member.is_dir();
        let dirs = if is_dir {
            Some(path.as_path())
        } else {
            path.parent()
        };
        for dir in dirs
            .into_iter()
            .flat_map(Path::ancestors)
            .take_while(|p| *p != root)
        {
            let dir = dir.to_str().ok_or(INVALID)?;
            if let Some(is_file) = existing(dir) {
                if is_file {
                    return Err(CONFLICT.into());
                }
            } else if pending.entry(dir.to_owned()).or_insert(None).is_some() {
                return Err(CONFLICT.into());
            }
            if pending.len() > MAX_ENTRIES {
                return Err(LIMIT.into());
            }
        }
        if !is_dir {
            let path = path.to_str().ok_or(INVALID)?;
            if existing(path).is_some() || pending.contains_key(path) {
                return Err(CONFLICT.into());
            }
            let size = member.size();
            if size > remaining || pending.len() >= MAX_ENTRIES {
                return Err(LIMIT.into());
            }
            let mut data = Vec::new();
            member
                .take(size + 1)
                .read_to_end(&mut data)
                .map_err(|_| INVALID)?;
            if data.len() as u64 != size {
                return Err(INVALID.into());
            }
            remaining -= size;
            pending.insert(path.to_owned(), Some(data));
        }
    }
    Ok(pending)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_selection_preserves_subdirectories_and_never_overwrites_a_destination_file() {
        let files = DemoFiles::seeded();
        let mut fs = files.borrow_mut();
        fs.put("/home/demo/log", None);
        fs.put("/home/demo/log/empty", None);
        fs.put("/home/demo/log/raw", Some(vec![0, 255, 1]));
        fs.put("/home/demo/log.old", Some(b"excluded".to_vec()));
        fs.put("/home/demo/saved.zip", Some(b"existing".to_vec()));
        let sources = ["/home/demo/log".into(), "/home/demo/notes.txt".into()];
        let path = fs
            .compress_zip(&sources, Path::new("/home/demo/saved.zip"))
            .unwrap();
        assert_eq!(path, Path::new("/home/demo/saved (1).zip"));
        let path = path.to_str().unwrap();
        assert_eq!(
            fs.entries["/home/demo/saved.zip"].data.as_deref(),
            Some(&b"existing"[..])
        );
        let zip = fs.entries[path].data.as_deref().unwrap();
        let info = oxideterm_local_files::list_archive_contents(Cursor::new(zip)).unwrap();
        let entries: Vec<_> = info
            .entries
            .iter()
            .map(|e| (e.path.as_str(), e.size))
            .collect();
        assert_eq!(
            entries,
            [
                ("log/", 0),
                ("log/empty/", 0),
                ("log/raw", 3),
                ("notes.txt", 10)
            ]
        );
        fs.put("/restored", None);
        fs.put("/restored/notes.txt", Some(b"keep".to_vec()));
        assert_eq!(fs.extract_zip(path, "/restored").unwrap_err(), CONFLICT);
        assert_eq!(
            fs.entries["/restored/notes.txt"].data.as_deref(),
            Some(&b"keep"[..])
        );
        assert!(!fs.entries.contains_key("/restored/log"));
        fs.entries.remove("/restored/notes.txt");
        fs.extract_zip(path, "/restored").unwrap();
        assert!(fs.entries["/restored/log/empty"].data.is_none());
        assert_eq!(
            fs.entries["/restored/log/raw"].data.as_deref(),
            Some(&[0, 255, 1][..])
        );
        assert_eq!(
            fs.entries["/restored/notes.txt"].data.as_deref(),
            Some(&b"OxideTerm\n"[..])
        );
    }
}
