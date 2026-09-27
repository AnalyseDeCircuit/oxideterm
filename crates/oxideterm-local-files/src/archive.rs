use std::path::Path;

use crate::{LocalArchiveEntry, LocalArchiveInfo};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveEntryOutcome {
    Completed(u64),
    Skipped,
    Failed,
}

fn report_failure<T, E>(
    result: Result<T, E>,
    path: &Path,
    progress: &mut impl FnMut(&Path, ArchiveEntryOutcome),
) -> Result<T, E> {
    if result.is_err() {
        progress(path, ArchiveEntryOutcome::Failed);
    }
    result
}

pub fn can_extract_archive(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    ["zip", "tar", "gz", "tgz", "tar.gz", "bz2", "xz", "7z"]
        .iter()
        .any(|ext| lower.ends_with(&format!(".{ext}")))
}

pub fn compress_local_files(
    files: &[String],
    archive_path: &str,
    progress: &mut impl FnMut(&Path, ArchiveEntryOutcome),
) -> Result<(), String> {
    use std::fs::File;
    use walkdir::WalkDir;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    let archive_path = Path::new(archive_path);
    if let Some(parent) = archive_path.parent() {
        report_failure(std::fs::create_dir_all(parent), archive_path, progress)
            .map_err(|error| format!("Failed to create directory: {error}"))?;
    }
    let file = report_failure(File::create(archive_path), archive_path, progress)
        .map_err(|error| format!("Failed to create archive: {error}"))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    for file_path in files {
        let path = Path::new(file_path);
        if !path.exists() {
            progress(path, ArchiveEntryOutcome::Skipped);
            continue;
        }
        let base_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file");
        if path.is_dir() {
            for entry in WalkDir::new(path) {
                let entry = entry.map_err(|error| {
                    progress(error.path().unwrap_or(path), ArchiveEntryOutcome::Failed);
                    format!("Failed to read directory: {error}")
                })?;
                let entry_path = entry.path();
                if entry.file_type().is_symlink() {
                    progress(entry_path, ArchiveEntryOutcome::Skipped);
                    continue;
                }
                let relative_path = entry_path
                    .strip_prefix(path.parent().unwrap_or(path))
                    .map_err(|error| {
                        progress(entry_path, ArchiveEntryOutcome::Failed);
                        format!("Failed to calculate relative path: {error}")
                    })?;
                let name = relative_path.to_string_lossy();
                if entry_path.is_dir() {
                    let dir_name = if name.ends_with('/') {
                        name.to_string()
                    } else {
                        format!("{name}/")
                    };
                    report_failure(zip.add_directory(&dir_name, options), entry_path, progress)
                        .map_err(|error| format!("Failed to add directory: {error}"))?;
                    progress(entry_path, ArchiveEntryOutcome::Completed(0));
                } else {
                    report_failure(
                        zip.start_file(name.to_string(), options),
                        entry_path,
                        progress,
                    )
                    .map_err(|error| format!("Failed to add file: {error}"))?;
                    let mut input = report_failure(File::open(entry_path), entry_path, progress)
                        .map_err(|error| format!("Failed to open file: {error}"))?;
                    let bytes =
                        report_failure(std::io::copy(&mut input, &mut zip), entry_path, progress)
                            .map_err(|error| format!("Failed to write file: {error}"))?;
                    progress(entry_path, ArchiveEntryOutcome::Completed(bytes));
                }
            }
        } else {
            report_failure(zip.start_file(base_name, options), path, progress)
                .map_err(|error| format!("Failed to add file: {error}"))?;
            let mut input = report_failure(File::open(path), path, progress)
                .map_err(|error| format!("Failed to open file: {error}"))?;
            let bytes = report_failure(std::io::copy(&mut input, &mut zip), path, progress)
                .map_err(|error| format!("Failed to write file: {error}"))?;
            progress(path, ArchiveEntryOutcome::Completed(bytes));
        }
    }
    report_failure(zip.finish(), archive_path, progress)
        .map_err(|error| format!("Failed to finalize archive: {error}"))?;
    Ok(())
}

pub fn extract_local_archive(
    archive_path: &str,
    dest_path: &str,
    progress: &mut impl FnMut(&Path, ArchiveEntryOutcome),
) -> Result<(), String> {
    use std::fs::{File, OpenOptions};
    use zip::ZipArchive;

    let archive_path = Path::new(archive_path);
    let dest_path = Path::new(dest_path);
    report_failure(std::fs::create_dir_all(dest_path), dest_path, progress)
        .map_err(|error| format!("Failed to create destination directory: {error}"))?;
    let file = report_failure(File::open(archive_path), archive_path, progress)
        .map_err(|error| format!("Failed to open archive: {error}"))?;
    let mut archive = report_failure(ZipArchive::new(file), archive_path, progress)
        .map_err(|error| format!("Failed to read archive: {error}"))?;

    for index in 0..archive.len() {
        let entry_id = format!("archive_entry_index={index}");
        let mut file = report_failure(archive.by_index(index), Path::new(&entry_id), progress)
            .map_err(|error| format!("Failed to read entry: {error}"))?;
        let outpath = match file.enclosed_name() {
            Some(path) => dest_path.join(path),
            None => {
                progress(Path::new(&entry_id), ArchiveEntryOutcome::Skipped);
                continue;
            }
        };
        if file.is_dir() {
            report_failure(std::fs::create_dir_all(&outpath), &outpath, progress)
                .map_err(|error| format!("Failed to create directory: {error}"))?;
            progress(&outpath, ArchiveEntryOutcome::Completed(0));
        } else {
            if let Some(parent) = outpath.parent()
                && !parent.exists()
            {
                report_failure(std::fs::create_dir_all(parent), &outpath, progress)
                    .map_err(|error| format!("Failed to create directory: {error}"))?;
            }
            let mut output = report_failure(
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&outpath),
                &outpath,
                progress,
            )
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    format!("Refusing to overwrite existing file: {}", outpath.display())
                } else {
                    format!("Failed to create file: {error}")
                }
            })?;
            let bytes = report_failure(std::io::copy(&mut file, &mut output), &outpath, progress)
                .map_err(|error| format!("Failed to write file: {error}"))?;
            progress(&outpath, ArchiveEntryOutcome::Completed(bytes));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                std::fs::set_permissions(&outpath, std::fs::Permissions::from_mode(mode)).ok();
            }
        }
    }
    Ok(())
}

pub fn list_local_archive_contents(path: &str) -> Result<LocalArchiveInfo, String> {
    use std::fs::File;
    use zip::ZipArchive;

    let file = File::open(path).map_err(|error| format!("Failed to open archive: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Failed to read archive: {error}"))?;
    let mut entries = Vec::new();
    let mut total_files = 0;
    let mut total_dirs = 0;
    let mut total_size = 0;
    let mut compressed_size = 0;

    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| format!("Failed to read entry {index}: {error}"))?;
        let name = file.name().to_string();
        let is_dir = file.is_dir();
        let size = file.size();
        let comp_size = file.compressed_size();
        let modified = file.last_modified().map(|dt| {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                dt.year(),
                dt.month(),
                dt.day(),
                dt.hour(),
                dt.minute(),
                dt.second()
            )
        });
        if is_dir {
            total_dirs += 1;
        } else {
            total_files += 1;
            total_size += size;
            compressed_size += comp_size;
        }
        let display_name = Path::new(&name)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| name.clone());
        entries.push(LocalArchiveEntry {
            name: display_name,
            path: name,
            is_dir,
            size,
            compressed_size: comp_size,
            modified,
        });
    }
    entries.sort_by(|left, right| match (left.is_dir, right.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => left.path.cmp(&right.path),
    });
    Ok(LocalArchiveInfo {
        entries,
        total_files,
        total_dirs,
        total_size,
        compressed_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn temp_root(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "oxideterm-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn compression_reports_written_bytes_and_skipped_source() {
        let root = temp_root("archive");
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("source.txt");
        let missing = root.join("missing.txt");
        let archive = root.join("out.zip");
        std::fs::write(&source, b"test").unwrap();
        let mut events = Vec::new();
        compress_local_files(
            &[
                source.to_string_lossy().into_owned(),
                missing.to_string_lossy().into_owned(),
            ],
            &archive.to_string_lossy(),
            &mut |path, outcome| events.push((path.to_path_buf(), outcome)),
        )
        .unwrap();

        assert_eq!(
            events,
            vec![
                (source, ArchiveEntryOutcome::Completed(4)),
                (missing, ArchiveEntryOutcome::Skipped)
            ]
        );
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&archive).unwrap()).unwrap();
        let mut content = String::new();
        zip.by_name("source.txt")
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert_eq!(content, "test");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extraction_reports_completed_and_failed_entries_without_overwriting() {
        let root = temp_root("extract");
        let destination = root.join("destination");
        std::fs::create_dir_all(&destination).unwrap();
        let archive = root.join("input.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("first.txt", options).unwrap();
        zip.write_all(b"new").unwrap();
        zip.start_file("second.txt", options).unwrap();
        zip.write_all(b"archive").unwrap();
        zip.finish().unwrap();
        std::fs::write(destination.join("second.txt"), b"existing").unwrap();

        let mut events = Vec::new();
        let result = extract_local_archive(
            &archive.to_string_lossy(),
            &destination.to_string_lossy(),
            &mut |path, outcome| events.push((path.to_path_buf(), outcome)),
        );
        assert!(result.is_err());
        assert_eq!(
            events,
            vec![
                (
                    destination.join("first.txt"),
                    ArchiveEntryOutcome::Completed(3)
                ),
                (destination.join("second.txt"), ArchiveEntryOutcome::Failed),
            ]
        );
        assert_eq!(
            std::fs::read(destination.join("first.txt")).unwrap(),
            b"new"
        );
        assert_eq!(
            std::fs::read(destination.join("second.txt")).unwrap(),
            b"existing"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
