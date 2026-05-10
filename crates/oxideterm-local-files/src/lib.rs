use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_PREVIEW_SIZE: u64 = 10 * 1024 * 1024;
pub const STREAM_PREVIEW_THRESHOLD: u64 = 256 * 1024;
pub const BOOKMARKS_FILENAME: &str = "oxideterm-file-bookmarks.json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalFileType {
    File,
    Directory,
    Symlink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalSortField {
    Name,
    Size,
    Modified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalSortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalClipboardMode {
    Copy,
    Cut,
}

#[derive(Clone, Debug)]
pub struct LocalFileEntry {
    pub name: String,
    pub path: String,
    pub file_type: LocalFileType,
    pub size: u64,
    pub modified: Option<i64>,
    pub readonly: bool,
    pub symlink_target: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LocalDrive {
    pub name: String,
    pub path: String,
    pub drive_type: String,
    pub total_space: u64,
    pub available_space: u64,
    pub read_only: bool,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct LocalBookmark {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(rename = "createdAt")]
    pub created_at: u64,
}

#[derive(Clone, Debug)]
pub enum LocalPreview {
    Loading,
    Text {
        content: String,
        language: Option<String>,
    },
    Markdown {
        content: String,
    },
    Image {
        path: String,
        mime_type: String,
    },
    Video {
        path: String,
        mime_type: String,
    },
    Audio {
        path: String,
        mime_type: String,
    },
    Font {
        path: String,
        mime_type: String,
    },
    Pdf {
        path: String,
        mime_type: String,
    },
    Archive {
        info: LocalArchiveInfo,
    },
    TooLarge {
        size: u64,
    },
    Unsupported(String),
    Error(String),
}

#[derive(Clone, Debug)]
pub struct LocalArchiveEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub compressed_size: u64,
    pub modified: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LocalArchiveInfo {
    pub entries: Vec<LocalArchiveEntry>,
    pub total_files: u64,
    pub total_dirs: u64,
    pub total_size: u64,
    pub compressed_size: u64,
}

#[derive(Clone, Debug)]
pub struct LocalPreviewMetadata {
    pub size: u64,
    pub modified: Option<i64>,
    pub created: Option<i64>,
    pub accessed: Option<i64>,
    pub readonly: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub mode: Option<u32>,
    pub mime_type: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LocalChecksumResult {
    pub md5: String,
    pub sha256: String,
}

pub fn home_path() -> String {
    std::env::var("HOME").unwrap_or_else(|_| {
        #[cfg(windows)]
        {
            "C:\\".to_string()
        }
        #[cfg(not(windows))]
        {
            "/".to_string()
        }
    })
}

pub fn default_file_manager_bookmarks_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(BOOKMARKS_FILENAME)
}

pub fn new_file_manager_bookmark_id() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("bookmark-{millis}")
}

pub fn bookmark_name_for_path(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    Path::new(trimmed)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string())
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

pub fn normalize_local_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed == "~" {
        return home_path();
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return Path::new(&home_path())
            .join(rest)
            .to_string_lossy()
            .to_string();
    }
    if trimmed.is_empty() {
        home_path()
    } else {
        trimmed.to_string()
    }
}

pub fn list_local_files(path: &str) -> std::io::Result<Vec<LocalFileEntry>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let path_buf = entry.path();
        let symlink_metadata = std::fs::symlink_metadata(&path_buf)?;
        let target_metadata = std::fs::metadata(&path_buf).ok();
        let metadata = target_metadata.as_ref().unwrap_or(&symlink_metadata);
        let file_type = if symlink_metadata.file_type().is_symlink() {
            LocalFileType::Symlink
        } else if metadata.is_dir() {
            LocalFileType::Directory
        } else {
            LocalFileType::File
        };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|mtime| mtime.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64);
        entries.push(LocalFileEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: path_buf.to_string_lossy().to_string(),
            file_type,
            size: if metadata.is_dir() { 0 } else { metadata.len() },
            modified,
            readonly: metadata.permissions().readonly(),
            symlink_target: std::fs::read_link(&path_buf)
                .ok()
                .map(|target| target.to_string_lossy().to_string()),
        });
    }
    entries.sort_by(local_file_default_cmp);
    Ok(entries)
}

pub fn local_file_default_cmp(left: &LocalFileEntry, right: &LocalFileEntry) -> std::cmp::Ordering {
    match (left.is_directory_like(), right.is_directory_like()) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
    }
}

pub fn sorted_local_files(
    files: &[LocalFileEntry],
    filter: &str,
    sort_field: LocalSortField,
    sort_direction: LocalSortDirection,
) -> Vec<LocalFileEntry> {
    let filter = filter.trim().to_lowercase();
    let mut filtered = files
        .iter()
        .filter(|file| filter.is_empty() || file.name.to_lowercase().contains(&filter))
        .cloned()
        .collect::<Vec<_>>();
    filtered.sort_by(|left, right| {
        if left.is_directory_like() && !right.is_directory_like() {
            return std::cmp::Ordering::Less;
        }
        if !left.is_directory_like() && right.is_directory_like() {
            return std::cmp::Ordering::Greater;
        }
        let ordering = match sort_field {
            LocalSortField::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
            LocalSortField::Size => left.size.cmp(&right.size),
            LocalSortField::Modified => left.modified.cmp(&right.modified),
        };
        match sort_direction {
            LocalSortDirection::Asc => ordering,
            LocalSortDirection::Desc => ordering.reverse(),
        }
    });
    filtered
}

impl LocalFileEntry {
    pub fn is_directory_like(&self) -> bool {
        self.file_type == LocalFileType::Directory
    }
}

pub fn local_parent_path(path: &str) -> Option<String> {
    let path = Path::new(path);
    path.parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .filter(|parent| !parent.is_empty())
}

pub fn join_local_path(base: &str, name: &str) -> String {
    Path::new(base).join(name).to_string_lossy().to_string()
}

pub fn validate_local_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("name is empty".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed == "." || trimmed == ".." {
        return Err("invalid name".to_string());
    }
    if trimmed.contains("..") {
        return Err("invalid name".to_string());
    }
    Ok(())
}

pub fn unique_copy_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "copy".to_string());
    let ext = path
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    for index in 1..=100 {
        let candidate = parent.join(format!("{stem} ({index}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem} (copy){ext}"))
}

pub fn copy_recursively(source: &Path, target: &Path) -> std::io::Result<()> {
    copy_recursively_with_progress(source, target, &mut |_| {})
}

pub fn local_operation_unit_count(path: &Path) -> usize {
    if !path.is_dir() {
        return 1;
    }
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path() != path)
        .count()
        .saturating_add(1)
}

pub fn copy_recursively_with_progress(
    source: &Path,
    target: &Path,
    progress: &mut impl FnMut(&Path),
) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(source)?;
    if metadata.is_dir() {
        std::fs::create_dir_all(target)?;
        progress(source);
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_recursively_with_progress(
                &entry.path(),
                &target.join(entry.file_name()),
                progress,
            )?;
        }
    } else {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, target)?;
        progress(source);
    }
    Ok(())
}

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

pub fn would_move_directory_into_itself(source: &Path, target: &Path) -> bool {
    let Ok(source) = source.canonicalize() else {
        return false;
    };
    let target = target
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .unwrap_or_else(|| target.to_path_buf());
    target.starts_with(source)
}

pub fn read_local_preview(path: &str) -> LocalPreview {
    let path_ref = Path::new(path);
    let Ok(metadata) = std::fs::metadata(path_ref) else {
        return LocalPreview::Error("Unable to read file metadata".to_string());
    };
    let file_name = path_ref
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = local_file_extension(&file_name);
    let file_size = metadata.len();

    if image_extensions().contains(&ext.as_str()) {
        if file_size > MAX_PREVIEW_SIZE {
            return LocalPreview::TooLarge { size: file_size };
        }
        return LocalPreview::Image {
            path: path.to_string(),
            mime_type: mime_type_for_extension(&ext),
        };
    }
    if video_extensions().contains(&ext.as_str()) {
        if file_size > MAX_PREVIEW_SIZE {
            return LocalPreview::TooLarge { size: file_size };
        }
        return LocalPreview::Video {
            path: path.to_string(),
            mime_type: mime_type_for_extension(&ext),
        };
    }
    if audio_extensions().contains(&ext.as_str()) {
        if file_size > MAX_PREVIEW_SIZE {
            return LocalPreview::TooLarge { size: file_size };
        }
        return LocalPreview::Audio {
            path: path.to_string(),
            mime_type: mime_type_for_extension(&ext),
        };
    }
    if font_extensions().contains(&ext.as_str()) {
        if file_size > MAX_PREVIEW_SIZE {
            return LocalPreview::TooLarge { size: file_size };
        }
        return LocalPreview::Font {
            path: path.to_string(),
            mime_type: mime_type_for_extension(&ext),
        };
    }
    if ext == "pdf" {
        if file_size > MAX_PREVIEW_SIZE {
            return LocalPreview::TooLarge { size: file_size };
        }
        return LocalPreview::Pdf {
            path: path.to_string(),
            mime_type: "application/pdf".to_string(),
        };
    }
    if archive_extensions().contains(&ext.as_str()) {
        return match list_local_archive_contents(path) {
            Ok(info) => LocalPreview::Archive { info },
            Err(_) => LocalPreview::Unsupported("fileManager.binaryFile".to_string()),
        };
    }
    if office_extensions().contains(&ext.as_str()) {
        return LocalPreview::Unsupported("fileManager.openExternal".to_string());
    }
    if file_size > MAX_PREVIEW_SIZE {
        return LocalPreview::TooLarge { size: file_size };
    }
    match std::fs::read(path) {
        Ok(bytes) if bytes.is_empty() => LocalPreview::Text {
            content: String::new(),
            language: language_for_extension(&ext, &file_name),
        },
        Ok(bytes) if looks_binary(&bytes) => {
            LocalPreview::Unsupported("fileManager.binaryFile".to_string())
        }
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => {
                if markdown_extensions().contains(&ext.as_str()) {
                    LocalPreview::Markdown { content: text }
                } else {
                    LocalPreview::Text {
                        content: text,
                        language: language_for_extension(&ext, &file_name),
                    }
                }
            }
            Err(error) => {
                let bytes = error.into_bytes();
                let text = String::from_utf8_lossy(&bytes).to_string();
                if looks_binary(text.as_bytes()) {
                    LocalPreview::Unsupported("fileManager.binaryFile".to_string())
                } else {
                    LocalPreview::Text {
                        content: text,
                        language: language_for_extension(&ext, &file_name),
                    }
                }
            }
        },
        Err(error) => LocalPreview::Error(error.to_string()),
    }
}

pub fn local_preview_metadata(path: &str) -> Option<LocalPreviewMetadata> {
    let path = Path::new(path);
    let metadata = std::fs::metadata(path).ok()?;
    let symlink_metadata = std::fs::symlink_metadata(path).ok();
    let is_symlink = symlink_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.file_type().is_symlink());
    let timestamp = |time: std::io::Result<std::time::SystemTime>| {
        time.ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
    };
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        Some(metadata.permissions().mode())
    };
    #[cfg(not(unix))]
    let mode = None;
    Some(LocalPreviewMetadata {
        size: metadata.len(),
        modified: timestamp(metadata.modified()),
        created: timestamp(metadata.created()),
        accessed: timestamp(metadata.accessed()),
        readonly: metadata.permissions().readonly(),
        is_dir: metadata.is_dir(),
        is_symlink,
        mode,
        mime_type: path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| mime_type_for_extension(&ext.to_lowercase())),
    })
}

pub fn calculate_local_checksum(path: &str) -> Result<LocalChecksumResult, String> {
    use sha2::{Digest, Sha256};

    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("Failed to open file for checksum: {error}"))?;
    let mut md5_context = md5::Context::new();
    let mut sha256 = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Failed to read file for checksum: {error}"))?;
        if read == 0 {
            break;
        }
        md5_context.consume(&buffer[..read]);
        sha256.update(&buffer[..read]);
    }
    Ok(LocalChecksumResult {
        md5: format!("{:x}", md5_context.compute()),
        sha256: format!("{:x}", sha256.finalize()),
    })
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

pub fn local_file_extension(file_name: &str) -> String {
    if file_name.starts_with('.') && !file_name[1..].contains('.') {
        return String::new();
    }
    Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .unwrap_or_default()
}

fn looks_binary(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(4096)];
    if sample.iter().filter(|byte| **byte == 0).count() > 10 {
        return true;
    }
    let non_printable = sample
        .iter()
        .filter(|byte| matches!(**byte, 0x00..=0x08 | 0x0e..=0x1f))
        .count();
    !sample.is_empty() && non_printable > sample.len() / 10
}

fn image_extensions() -> &'static [&'static str] {
    &["png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp"]
}

fn video_extensions() -> &'static [&'static str] {
    &["mp4", "webm", "ogv", "mov", "mkv", "avi", "m4v"]
}

fn audio_extensions() -> &'static [&'static str] {
    &["mp3", "wav", "ogg", "flac", "aac", "m4a", "wma", "opus"]
}

fn font_extensions() -> &'static [&'static str] {
    &["ttf", "otf", "woff", "woff2", "eot"]
}

fn office_extensions() -> &'static [&'static str] {
    &[
        "docx", "xlsx", "pptx", "doc", "xls", "ppt", "odt", "ods", "odp",
    ]
}

fn archive_extensions() -> &'static [&'static str] {
    &["zip", "jar", "war", "ear", "apk", "xpi", "crx", "epub"]
}

fn markdown_extensions() -> &'static [&'static str] {
    &["md", "markdown", "mdx"]
}

fn shell_config_file(file_name: &str) -> bool {
    matches!(
        file_name,
        ".bashrc"
            | ".bash_profile"
            | ".bash_login"
            | ".bash_logout"
            | ".bash_aliases"
            | ".zshrc"
            | ".zshenv"
            | ".zprofile"
            | ".zlogin"
            | ".zlogout"
            | ".profile"
            | ".tcshrc"
            | ".cshrc"
            | ".kshrc"
            | ".fishrc"
            | ".vimrc"
            | ".gvimrc"
            | ".exrc"
            | ".nanorc"
            | ".gitconfig"
            | ".gitignore"
            | ".gitattributes"
            | ".editorconfig"
            | ".prettierrc"
            | ".eslintrc"
            | ".stylelintrc"
            | ".npmrc"
            | ".yarnrc"
            | ".nvmrc"
            | ".node-version"
            | ".python-version"
            | ".env"
            | ".env.local"
            | ".env.development"
            | ".env.production"
            | ".htaccess"
            | ".dockerignore"
            | ".hgignore"
            | "Makefile"
            | "Dockerfile"
            | "Vagrantfile"
            | "Procfile"
            | "Gemfile"
            | "Rakefile"
            | "CMakeLists.txt"
            | "Cargo.toml"
            | "package.json"
            | "tsconfig.json"
    )
}

fn language_for_extension(ext: &str, file_name: &str) -> Option<String> {
    if shell_config_file(file_name) {
        return Some("bash".to_string());
    }
    let language = match ext {
        "js" => "javascript",
        "jsx" => "jsx",
        "ts" => "typescript",
        "tsx" => "tsx",
        "py" => "python",
        "rs" => "rust",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "kt" => "kotlin",
        "scala" => "scala",
        "sh" | "bash" | "zsh" => "bash",
        "fish" => "fish",
        "ps1" => "powershell",
        "sql" => "sql",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "sass" => "sass",
        "less" => "less",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "vue" => "vue",
        "svelte" => "svelte",
        _ => return None,
    };
    Some(language.to_string())
}

pub fn mime_type_for_extension(ext: &str) -> String {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" | "aac" => "audio/mp4",
        "wma" => "audio/x-ms-wma",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "zip" | "jar" | "war" | "ear" | "apk" | "xpi" | "crx" | "epub" => "application/zip",
        "js" => "text/javascript",
        "ts" => "text/typescript",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "md" | "markdown" | "mdx" => "text/markdown",
        "txt" | "log" | "ini" | "conf" | "cfg" | "env" => "text/plain",
        "py" => "text/x-python",
        "rs" => "text/x-rust",
        "go" => "text/x-go",
        "java" => "text/x-java",
        "c" | "h" => "text/x-c",
        "cpp" | "hpp" | "cc" => "text/x-c++",
        "sh" | "bash" | "zsh" => "text/x-shellscript",
        "yaml" | "yml" => "text/yaml",
        "toml" => "text/x-toml",
        _ => "application/octet-stream",
    }
    .to_string()
}

pub fn local_drives() -> Vec<LocalDrive> {
    let mut drives = platform_local_drives();
    drives.sort_by(|left, right| {
        let left_system = if left.drive_type == "system" { 0 } else { 1 };
        let right_system = if right.drive_type == "system" { 0 } else { 1 };
        left_system
            .cmp(&right_system)
            .then_with(|| left.path.cmp(&right.path))
    });
    if drives.is_empty() {
        drives.push(LocalDrive {
            name: "System".to_string(),
            path: home_path_root(),
            drive_type: "system".to_string(),
            total_space: 0,
            available_space: 0,
            read_only: false,
        });
    }
    drives
}

fn home_path_root() -> String {
    #[cfg(windows)]
    {
        "C:\\".to_string()
    }
    #[cfg(not(windows))]
    {
        "/".to_string()
    }
}

fn platform_local_drives() -> Vec<LocalDrive> {
    use sysinfo::Disks;

    let disks = Disks::new_with_refreshed_list();
    let mut drives: Vec<LocalDrive> = Vec::new();

    #[cfg(unix)]
    let mut seen_dev_ids: HashMap<u64, usize> = HashMap::new();
    #[cfg(not(unix))]
    let mut seen_mount_points: HashSet<PathBuf> = HashSet::new();

    for disk in disks.list() {
        let mount_point = disk.mount_point().to_path_buf();

        #[cfg(unix)]
        let unix_dev_id = {
            use std::os::unix::fs::MetadataExt;
            match std::fs::metadata(&mount_point) {
                Ok(metadata) => {
                    let dev = metadata.dev();
                    if let Some(&existing_idx) = seen_dev_ids.get(&dev) {
                        if mount_point.as_os_str().len() < drives[existing_idx].path.len() {
                            drives[existing_idx].path = mount_point.to_string_lossy().to_string();
                            drives[existing_idx].name = drive_display_name(disk, &mount_point);
                        }
                        continue;
                    }
                    Some(dev)
                }
                Err(_) => None,
            }
        };
        #[cfg(not(unix))]
        {
            let canonical = mount_point
                .canonicalize()
                .unwrap_or_else(|_| mount_point.clone());
            if !seen_mount_points.insert(canonical) {
                continue;
            }
        }

        let mount = mount_point.to_string_lossy();
        if is_pseudo_mount(&mount) {
            continue;
        }

        #[cfg(unix)]
        if let Some(dev) = unix_dev_id {
            seen_dev_ids.insert(dev, drives.len());
        }

        let read_only = if cfg!(target_os = "macos") && mount == "/" {
            !std::fs::metadata("/Users")
                .map(|metadata| !metadata.permissions().readonly())
                .unwrap_or(false)
        } else {
            disk.is_read_only()
        };

        drives.push(LocalDrive {
            name: drive_display_name(disk, &mount_point),
            path: mount.to_string(),
            drive_type: classify_disk(disk).to_string(),
            total_space: disk.total_space(),
            available_space: disk.available_space(),
            read_only,
        });
    }
    drives
}

fn is_pseudo_mount(mount: &str) -> bool {
    mount.starts_with("/proc")
        || mount.starts_with("/sys")
        || mount.starts_with("/dev")
        || mount.starts_with("/snap")
        || mount == "/boot"
        || mount == "/boot/efi"
        || is_blocked_run_mount(mount)
}

fn is_blocked_run_mount(mount: &str) -> bool {
    if !mount.starts_with("/run") {
        return false;
    }
    if mount.starts_with("/run/media/") || mount.starts_with("/run/mount/") {
        return false;
    }
    mount.starts_with("/run/user/") && !mount.contains("/gvfs/")
        || (!mount.starts_with("/run/user/"))
}

fn drive_display_name(disk: &sysinfo::Disk, mount_point: &Path) -> String {
    let raw_name = disk.name().to_string_lossy().to_string();
    if !raw_name.is_empty() {
        return raw_name;
    }
    let mount = mount_point.to_string_lossy();
    mount_point
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            if mount == "/" {
                "System".to_string()
            } else {
                mount.to_string()
            }
        })
}

fn classify_disk(disk: &sysinfo::Disk) -> &'static str {
    use sysinfo::DiskKind;

    let mount = disk.mount_point().to_string_lossy();
    #[cfg(not(windows))]
    if mount == "/" {
        return "system";
    }
    #[cfg(windows)]
    if mount.to_uppercase().starts_with("C:") {
        return "system";
    }
    if mount.contains("://") || mount.starts_with("//") {
        return "network";
    }
    match disk.kind() {
        DiskKind::HDD | DiskKind::SSD => "local",
        _ => "removable",
    }
}

pub fn directory_stats(path: &Path) -> (u64, u64, u64) {
    let mut files = 0;
    let mut dirs = 0;
    let mut size = 0;
    let Ok(entries) = std::fs::read_dir(path) else {
        return (files, dirs, size);
    };
    for entry in entries.flatten() {
        let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if metadata.is_dir() {
            dirs += 1;
            let nested = directory_stats(&entry.path());
            files += nested.0;
            dirs += nested.1;
            size += nested.2;
        } else {
            files += 1;
            size += metadata.len();
        }
    }
    (files, dirs, size)
}
