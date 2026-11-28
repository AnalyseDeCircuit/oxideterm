use super::*;
use crate::{
    browser_file::BrowserImport,
    demo::{MAX_ARCHIVE_BYTES, MAX_ARCHIVE_ENTRIES, unpack_zip},
};
use std::{
    collections::{BTreeMap, HashMap},
    io::{Cursor, Write},
};

const INVALID: &str = "fileManager.archiveInvalid";
const LIMIT: &str = "fileManager.archiveLimit";
const CONFLICT: &str = "sftp.conflict.description";

fn split(path: &Path) -> Result<(&str, &str), String> {
    let parent = path
        .parent()
        .and_then(Path::to_str)
        .ok_or("ide.validation.nameInvalid")?;
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("ide.validation.nameInvalid")?;
    validate_filename(name)?;
    Ok((parent, name))
}

async fn names(root: &DeviceFolder, dir: &str) -> Result<HashSet<String>, String> {
    Ok(entries(root, dir)
        .await?
        .into_iter()
        .map(|e| e.name)
        .collect())
}

fn available(dir: &str, name: &str, used: &HashSet<String>) -> PathBuf {
    unique_copy_path_with(&Path::new(dir).join(name), |p| {
        p.file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| used.contains(n))
    })
}

pub(super) async fn batch(
    root: DeviceFolder,
    kind: FileBatch,
    paths: Vec<String>,
    progress: BatchProgress,
) -> Result<(), String> {
    let (destination, cut) = match kind {
        FileBatch::BrowserDrop {
            destination,
            payload,
        } => {
            let dir = root.directory(&destination).await.map_err(err)?;
            let mut existing = names(&dir, "/").await?;
            let mut reserved: HashSet<_> = payload.0.iter().map(|item| item.name()).collect();
            for (index, item) in payload.0.into_iter().enumerate() {
                let mut import = BrowserImport::open(item).await?;
                while !import.complete() {
                    import.read_chunk().await?;
                }
                reserved.remove(&import.name);
                let used = existing.union(&reserved).cloned().collect();
                let path = available("/", &import.name, &used);
                let (_, name) = split(&path)?;
                for (relative, data) in import.take_tree() {
                    let to = if relative.is_empty() {
                        child("/", name)?
                    } else {
                        format!("/{name}/{relative}")
                    };
                    if let Some(bytes) = data {
                        dir.save_bytes(&to, &bytes, None, true).await.map_err(err)?;
                    } else {
                        dir.create_folder(String::new(), to).await.map_err(err)?;
                    }
                }
                existing.insert(name.to_owned());
                progress(LocalBatchProgress {
                    current: index + 1,
                    file_name: name.to_owned(),
                });
            }
            return Ok(());
        }
        FileBatch::Paste { destination, mode } => {
            (Some(destination), mode == LocalClipboardMode::Cut)
        }
        FileBatch::Duplicate => (None, false),
    };
    for (index, from) in paths.iter().enumerate() {
        let (parent, name) = split(Path::new(from))?;
        let dir = destination.as_deref().unwrap_or(parent);
        let to = available(dir, name, &names(&root, dir).await?)
            .to_str()
            .ok_or("ide.validation.nameInvalid")?
            .to_owned();
        if cut {
            root.rename_item(String::new(), from.clone(), to)
                .await
                .map_err(err)?;
        } else {
            root.copy_item(String::new(), from.clone(), to)
                .await
                .map_err(err)?;
        }
        progress(LocalBatchProgress {
            current: index + 1,
            file_name: name.to_owned(),
        });
    }
    Ok(())
}

pub(super) async fn compress(
    root: &DeviceFolder,
    paths: Vec<String>,
    to: &Path,
) -> Result<PathBuf, String> {
    let (parent, name) = split(to)?;
    let dest = root.directory(parent).await.map_err(err)?;
    let rows = entries(&dest, "/").await?;
    let used = rows.iter().map(|e| e.name.clone()).collect();
    let output = available(parent, name, &used);
    let mut work = Vec::new();
    for path in paths {
        let (src, leaf) = split(Path::new(&path))?;
        if src != parent {
            return Err(INVALID.into());
        }
        let entry = rows.iter().find(|e| e.name == leaf).ok_or("ENOENT")?;
        work.push((leaf.to_owned(), entry.clone()));
    }
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut bytes = 0;
    let mut count = 0;
    while let Some((name, entry)) = work.pop() {
        count += 1;
        if count > MAX_ARCHIVE_ENTRIES {
            return Err(LIMIT.into());
        }
        if entry.is_directory_like() {
            zip.add_directory(&name, options).map_err(|_| INVALID)?;
            let children = entries(&dest, &entry.path).await?;
            if count + work.len() + children.len() > MAX_ARCHIVE_ENTRIES {
                return Err(LIMIT.into());
            }
            work.extend(
                children
                    .into_iter()
                    .map(|e| (format!("{name}/{}", e.name), e)),
            );
        } else {
            let file = dest.snapshot(&entry.path).await.map_err(err)?;
            let size = file.size() as u64;
            bytes += size;
            if bytes > MAX_ARCHIVE_BYTES {
                return Err(LIMIT.into());
            }
            zip.start_file(&name, options).map_err(|_| INVALID)?;
            for start in (0..size).step_by(256 * 1024) {
                zip.write_all(&slice(&file, start, (start + 256 * 1024).min(size)).await?)
                    .map_err(|_| INVALID)?;
            }
        }
    }
    let archive = zip.finish().map_err(|_| INVALID)?.into_inner();
    if archive.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err(LIMIT.into());
    }
    dest.save_bytes(&child("/", split(&output)?.1)?, &archive, None, true)
        .await
        .map_err(err)?;
    Ok(output)
}

pub(super) async fn extract(
    root: &DeviceFolder,
    path: &str,
    destination: &str,
) -> Result<(), String> {
    let file = root.snapshot(path).await.map_err(err)?;
    if file.size() as u64 > MAX_ARCHIVE_BYTES {
        return Err(LIMIT.into());
    }
    let bytes = slice(&file, 0, file.size() as u64).await?;
    let decoded = unpack_zip(&bytes, "/", |_| None)?;
    drop(bytes);
    let dest = root.directory(destination).await.map_err(err)?;
    write_archive(dest, decoded).await
}

async fn write_archive(
    root: DeviceFolder,
    tree: BTreeMap<String, Option<Vec<u8>>>,
) -> Result<(), String> {
    // Validate every existing destination before creating entries. New files use create-only writes.
    let mut children = HashMap::from([("/".to_string(), entries(&root, "/").await?)]);
    let mut dirs = HashMap::from([("/".to_string(), root)]);
    for (path, data) in &tree {
        let (parent, name) = split(Path::new(path))?;
        if let Some(e) = children
            .get(parent)
            .and_then(|rows| rows.iter().find(|e| e.name == name))
        {
            if data.is_some() || !e.is_directory_like() {
                return Err(CONFLICT.into());
            }
            let dir = dirs[parent]
                .directory(&child("/", name)?)
                .await
                .map_err(err)?;
            children.insert(path.clone(), entries(&dir, "/").await?);
            dirs.insert(path.clone(), dir);
        }
    }
    for (path, data) in tree {
        let (parent, name) = split(Path::new(&path))?;
        let pinned = dirs.get(parent).ok_or(INVALID)?;
        let local = child("/", name)?;
        if let Some(bytes) = data {
            pinned
                .save_bytes(&local, &bytes, None, true)
                .await
                .map_err(err)?;
        } else if !dirs.contains_key(&path) {
            pinned
                .create_folder(String::new(), local.clone())
                .await
                .map_err(err)?;
            let dir = pinned.directory(&local).await.map_err(err)?;
            dirs.insert(path, dir);
        }
    }
    Ok(())
}
