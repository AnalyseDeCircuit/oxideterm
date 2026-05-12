use crate::{LocalFileEntry, LocalFileType, LocalSortDirection, LocalSortField};

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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, file_type: LocalFileType, size: u64) -> LocalFileEntry {
        LocalFileEntry {
            name: name.to_string(),
            path: format!("/tmp/{name}"),
            file_type,
            size,
            modified: None,
            readonly: false,
            symlink_target: None,
        }
    }

    #[test]
    fn sorted_files_keep_directories_before_files() {
        let files = vec![
            entry("b.txt", LocalFileType::File, 2),
            entry("a-dir", LocalFileType::Directory, 0),
            entry("a.txt", LocalFileType::File, 1),
        ];
        let sorted = sorted_local_files(&files, "", LocalSortField::Name, LocalSortDirection::Asc);
        let names = sorted
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["a-dir", "a.txt", "b.txt"]);
    }
}
