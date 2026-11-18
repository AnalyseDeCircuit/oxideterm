use super::{LocalFileEntry, LocalFileType, directory_stats, local_preview_metadata};

#[derive(Clone, Debug)]
pub struct LocalFileProperties {
    pub location: String,
    pub size: u64,
    pub modified: Option<i64>,
    pub accessed: Option<i64>,
    pub readonly: bool,
    pub dir_files: Option<u64>,
    pub dir_dirs: Option<u64>,
    pub total_size: Option<u64>,
    pub created: Option<i64>,
    pub mode: Option<u32>,
    pub mime_type: Option<String>,
    pub is_symlink: bool,
}
pub fn local_file_properties(entry: &LocalFileEntry) -> LocalFileProperties {
    let metadata = local_preview_metadata(&entry.path);
    let accessed = metadata.as_ref().and_then(|metadata| metadata.accessed);
    let created = metadata.as_ref().and_then(|metadata| metadata.created);
    let mode = metadata.as_ref().and_then(|metadata| metadata.mode);
    let mime_type = metadata
        .as_ref()
        .and_then(|metadata| metadata.mime_type.clone());
    let is_symlink = metadata
        .as_ref()
        .is_some_and(|metadata| metadata.is_symlink);
    let (dir_files, dir_dirs, total_size) = if entry.file_type == LocalFileType::Directory {
        let stats = directory_stats(std::path::Path::new(&entry.path));
        (Some(stats.0), Some(stats.1), Some(stats.2))
    } else {
        (None, None, None)
    };
    let location = std::path::Path::new(&entry.path)
        .parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .unwrap_or_default();
    LocalFileProperties {
        location,
        size: entry.size,
        modified: entry.modified,
        accessed,
        readonly: entry.readonly,
        dir_files,
        dir_dirs,
        total_size,
        created,
        mode,
        mime_type,
        is_symlink,
    }
}
