use std::path::Path;
#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;

pub const BOOKMARKS_FILENAME: &str = "oxideterm-file-bookmarks.json";

#[cfg(not(target_family = "wasm"))]
pub fn default_file_manager_bookmarks_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(BOOKMARKS_FILENAME)
}

pub fn new_file_manager_bookmark_id() -> String {
    let millis = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
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
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(not(target_family = "wasm"))]
use super::LocalBookmark;

#[cfg(not(target_family = "wasm"))]
pub fn load_local_bookmarks(path: &Path) -> Vec<LocalBookmark> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

#[cfg(not(target_family = "wasm"))]
pub fn save_local_bookmarks(path: &Path, bookmarks: &[LocalBookmark]) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(bookmarks).map_err(|error| error.to_string())?;
    oxideterm_atomic_file::durable_write(path, &bytes).map_err(|error| error.to_string())
}

#[cfg(not(target_family = "wasm"))]
pub fn reorder_local_bookmarks(
    bookmarks: &mut Vec<LocalBookmark>,
    path: &Path,
    source: &str,
    target: &str,
    after: bool,
) -> Result<bool, String> {
    let Some(from) = bookmarks.iter().position(|bookmark| bookmark.id == source) else {
        return Ok(false);
    };
    let Some(target) = bookmarks.iter().position(|bookmark| bookmark.id == target) else {
        return Ok(false);
    };
    if from == target {
        return Ok(false);
    }
    let slot = target + usize::from(after);
    let to = slot - usize::from(from < slot);
    if from == to {
        return Ok(false);
    }
    let previous = bookmarks.clone();
    let moved = bookmarks.remove(from);
    bookmarks.insert(to, moved);
    if let Err(error) = save_local_bookmarks(path, bookmarks) {
        *bookmarks = previous;
        return Err(error);
    }
    Ok(true)
}
