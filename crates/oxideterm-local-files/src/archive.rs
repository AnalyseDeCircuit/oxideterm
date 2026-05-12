use std::path::Path;

use crate::{LocalArchiveEntry, LocalArchiveInfo};

pub fn can_extract_archive(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    ["zip", "tar", "gz", "tgz", "tar.gz", "bz2", "xz", "7z"]
        .iter()
        .any(|ext| lower.ends_with(&format!(".{ext}")))
}

pub fn compress_local_files(files: &[String], archive_path: &str) -> Result<(), String> {
    use std::fs::File;
    use walkdir::WalkDir;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    let archive_path = Path::new(archive_path);
    if let Some(parent) = archive_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create directory: {error}"))?;
    }
    let file =
        File::create(archive_path).map_err(|error| format!("Failed to create archive: {error}"))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    for file_path in files {
        let path = Path::new(file_path);
        if !path.exists() {
            continue;
        }
        let base_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file");
        if path.is_dir() {
            for entry in WalkDir::new(path) {
                let entry = entry.map_err(|error| format!("Failed to read directory: {error}"))?;
                let entry_path = entry.path();
                if entry.file_type().is_symlink() {
                    continue;
                }
                let relative_path = entry_path
                    .strip_prefix(path.parent().unwrap_or(path))
                    .map_err(|error| format!("Failed to calculate relative path: {error}"))?;
                let name = relative_path.to_string_lossy();
                if entry_path.is_dir() {
                    let dir_name = if name.ends_with('/') {
                        name.to_string()
                    } else {
                        format!("{name}/")
                    };
                    zip.add_directory(&dir_name, options)
                        .map_err(|error| format!("Failed to add directory: {error}"))?;
                } else {
                    zip.start_file(name.to_string(), options)
                        .map_err(|error| format!("Failed to add file: {error}"))?;
                    let mut input = File::open(entry_path)
                        .map_err(|error| format!("Failed to open file: {error}"))?;
                    std::io::copy(&mut input, &mut zip)
                        .map_err(|error| format!("Failed to write file: {error}"))?;
                }
            }
        } else {
            zip.start_file(base_name, options)
                .map_err(|error| format!("Failed to add file: {error}"))?;
            let mut input =
                File::open(path).map_err(|error| format!("Failed to open file: {error}"))?;
            std::io::copy(&mut input, &mut zip)
                .map_err(|error| format!("Failed to write file: {error}"))?;
        }
    }
    zip.finish()
        .map_err(|error| format!("Failed to finalize archive: {error}"))?;
    Ok(())
}

pub fn extract_local_archive(archive_path: &str, dest_path: &str) -> Result<(), String> {
    use std::fs::{File, OpenOptions};
    use zip::ZipArchive;

    let archive_path = Path::new(archive_path);
    let dest_path = Path::new(dest_path);
    std::fs::create_dir_all(dest_path)
        .map_err(|error| format!("Failed to create destination directory: {error}"))?;
    let file =
        File::open(archive_path).map_err(|error| format!("Failed to open archive: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Failed to read archive: {error}"))?;

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| format!("Failed to read entry: {error}"))?;
        let outpath = match file.enclosed_name() {
            Some(path) => dest_path.join(path),
            None => continue,
        };
        if file.is_dir() {
            std::fs::create_dir_all(&outpath)
                .map_err(|error| format!("Failed to create directory: {error}"))?;
        } else {
            if let Some(parent) = outpath.parent()
                && !parent.exists()
            {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("Failed to create directory: {error}"))?;
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&outpath)
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        format!("Refusing to overwrite existing file: {}", outpath.display())
                    } else {
                        format!("Failed to create file: {error}")
                    }
                })?;
            std::io::copy(&mut file, &mut output)
                .map_err(|error| format!("Failed to write file: {error}"))?;
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
